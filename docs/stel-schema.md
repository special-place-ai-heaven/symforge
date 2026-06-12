# STEL schema & layer specification

Companion to [`stel-architecture.md`](stel-architecture.md). **Define this first; implement second.**  
Every struct below is a contract between layers. If a component cannot emit/consume these types, it is **axed**.

---

## Survival rule

```text
superior   → survives (merged, default path)
inferior   → axed (removed, not deprecated limbo)
unknown    → no merge until battery + path proof
forward    → only v8 pinned baseline diff PASS moves the branch (from 8.0 tag)
assumption → VALIDATED before it unlocks the next phase; else research
```

No feature flags for dead approaches. No “keep old router behind env.” One winning path per concern.

**Assumption gate:** see [`stel-assumptions.md`](stel-assumptions.md). Unvalidated beliefs do not ship.

---

## System overview

```text
                    ┌─────────────────────────────────────────┐
  MCP tools/call    │ L0  SURFACE REGISTRY                    │
  ───────────────►  │  symforge | symforge_edit | status      │
                    └──────────────────┬──────────────────────┘
                                       │ StelRequest
                                       ▼
                    ┌─────────────────────────────────────────┐
                    │ L1  ROUTER                            │
                    │  IntentClassifier → PlanBuilder         │
                    └──────────────────┬──────────────────────┘
                                       │ StelPlan (draft)
                                       ▼
                    ┌─────────────────────────────────────────┐
                    │ L2  CONTROLLER                          │
                    │  Estimator → Admission → Decision       │
                    │  PlanExecutor (orchestrates L3 steps)     │
                    └──────────────────┬──────────────────────┘
                                       │ StelDecision + steps
                                       ▼
                    ┌─────────────────────────────────────────┐
                    │ L3  CORE TOOLS (32 handlers, internal)  │
                    └──────────────────┬──────────────────────┘
                                       │ StelStepResult[]
                                       ▼
                    ┌─────────────────────────────────────────┐
                    │ L4  LEDGER + CALIBRATION                │
                    │  SessionContext, TokenStats, Analytics    │
                    │  CalibrationEngine → adjusts L2 fudge   │
                    └──────────────────┬──────────────────────┘
                                       │
                                       ▼
                              StelResponse (trust envelope + body)
```

**Single L0 entry for read/explore:** `symforge` runs L1→L2→L3→L4 in one MCP round-trip.

---

## Layer contracts

| Layer | Module (proposed) | Consumes | Produces | Existing code |
|-------|-------------------|----------|----------|---------------|
| **L0** | `stel/surface.rs` | MCP args | `StelRequest` | `tools.rs` `ask`, `#[tool_handler]` |
| **L1** | `stel/router.rs` | `StelRequest`, index RO | `StelPlan` | `smart_query.rs` |
| **L2** | `stel/controller.rs` | `StelPlan`, index, session | `StelDecision` | new (+ `format.rs` baselines) |
| **L2** | `stel/executor.rs` | `StelDecision` | `StelExecution` | `tools.rs` handlers (internal call) |
| **L3** | `protocol/tools.rs` | core tool args | raw string + bytes | 32 `#[tool]` fns |
| **L4** | `stel/ledger.rs` | `StelExecution` | `StelLedgerEvent`, aggregates | `session.rs`, `TokenStats`, analytics |
| **L4** | `stel/calibration.rs` | ledger history | `CalibrationPatch` | new |

---

## Shared enums

### `IntentBucket` (L0 optional hint → L1)

```text
orient   — repo map, explore, conventions
find     — search_symbols, search_files, search_text
read     — get_symbol, get_file_context, get_file_content
trace    — find_references, get_symbol_context
impact   — find_dependents, what_changed, analyze_file_impact, diff_symbols
edit     — (routed to symforge_edit, not symforge)
meta     — context_inventory, investigation_suggest, tool catalog
auto     — default; L1 classifies from query (today's ask behavior)
```

Maps 1:1 from `QueryIntent` where possible; `auto` preserves NL-only callers.

### `RouteConfidence`

```text
exact | inferred | fallback
```

(from existing `smart_query::RouteConfidence`)

### `AdmissionDecision` (L2)

```text
serve      — run plan at full or degraded fidelity
degrade    — run plan with tighter caps / fewer sections
bypass     — do not run L3; return StelBypassBody
cache_hit  — do not run L3; return StelCacheBody
reject     — invalid request; no silent fallback to expensive path
```

### `CoreToolName`

Closed set = registered MCP tools (32 today). Plan steps reference this enum, not free strings.

---

## `StelRequest` (L0 → L1)

MCP input schema for `symforge` (JSON Schema source of truth for compact surface).

```json
{
  "type": "object",
  "properties": {
    "query": { "type": "string", "description": "Natural language or symbol/path token" },
    "intent": {
      "type": "string",
      "enum": ["auto", "orient", "find", "read", "trace", "impact", "meta"]
    },
    "path": { "type": "string" },
    "symbol": { "type": "string" },
    "max_tokens": { "type": "integer", "minimum": 64 },
    "preview": { "type": "boolean", "description": "If true, run L1+L2 only; return StelEstimate" }
  },
  "required": ["query"]
}
```

| Field | Role |
|-------|------|
| `query` | Required; fed to `classify_intent_with_match` when `intent=auto` |
| `intent` | Optional bucket override |
| `path` / `symbol` | Disambiguation; reduces L1 inference error |
| `max_tokens` | Hard ceiling on **response**; L2 may set lower |
| `preview` | Pre-flight: estimate tokens & decision without L3 execution |

---

## `StelPlan` (L1 → L2)

Draft execution plan **before** economics gate.

```json
{
  "plan_id": "uuid-v4",
  "intent": "trace",
  "confidence": "exact",
  "confidence_rationale": "matched explicit caller phrasing",
  "steps": [
    {
      "order": 1,
      "tool": "find_references",
      "args": { "name": "hard_link", "limit": 20, "compact": true },
      "est_response_tokens": 420,
      "est_manual_tokens": 800,
      "index_refs": [{ "path": "tokio/src/fs/hard_link.rs", "raw_chars": 3200 }]
    }
  ],
  "suggested_followup": null
}
```

| Field | Rule |
|-------|------|
| `steps` | 1..N; N>1 only when chain beats single step on **estimated** net |
| `est_*` | From index bytes/symbol counts; never from actual run |
| `index_refs` | Files touched; drives manual baseline sum |

**L1 axiom:** if `confidence=fallback` and plan is multi-step, L2 defaults stricter caps.

---

## `StelEstimate` (L2 preview — `preview: true`)

```json
{
  "plan_id": "…",
  "decision": "serve",
  "predicted_response_tokens": 420,
  "predicted_manual_tokens": 800,
  "predicted_schema_tokens": 45,
  "predicted_invoke_tokens": 80,
  "predicted_net_vs_manual": 380,
  "recommended": true
}
```

`recommended: false` when predicted net ≤ 0 → expect `decision: bypass` on full call.

---

## `StelDecision` (L2 → executor)

```json
{
  "plan_id": "…",
  "decision": "serve",
  "decision_reason": "predicted_net=380 > margin=50",
  "effective_max_tokens": 1000,
  "degrade_flags": ["outline_only", "no_hints"],
  "steps": [ "…same as plan, args may be tightened…" ],
  "bypass": null
}
```

When `decision=bypass`:

```json
{
  "decision": "bypass",
  "bypass": {
    "action": "host_read",
    "path": "tokio/src/fs/hard_link.rs",
    "start_line": 1,
    "end_line": 45,
    "predicted_manual_tokens": 320,
    "predicted_symforge_tokens": 381,
    "reason": "file_lines<=50; schema_overhead dominates payload"
  }
}
```

When `decision=cache_hit`:

```json
{
  "decision": "cache_hit",
  "cache": {
    "kind": "symbol",
    "path": "…",
    "name": "…",
    "prior_tokens": 352,
    "session_age_secs": 120
  }
}
```

---

## `StelExecution` (executor → L4)

```json
{
  "plan_id": "…",
  "decision": "serve",
  "steps_executed": [
    {
      "tool": "find_references",
      "success": true,
      "response_bytes": 1680,
      "response_tokens": 420,
      "duration_ms": 12
    }
  ],
  "body": "…merged payload for LLM…",
  "totals": {
    "response_tokens": 420,
    "manual_baseline_tokens": 800,
    "net_vs_manual": 380,
    "schema_tokens": 45,
    "invoke_tokens": 80
  }
}
```

Every execution **must** populate `totals` for ledger; bypass/cache use zero L3 steps but still record economics.

---

## `StelTrustEnvelope` (L0 response header)

Prepended to every `symforge` body (text or structured). Parsed by hosts; displayed to LLM.

```text
── stel ──
plan: trace → find_references (exact)
decision: serve
tokens: 420 served · 380 saved vs manual · schema 45 · invoke 80
predicted: 400 · error: 5.0%
session_net_vs_manual: +1240
calibration: ok
──
```

Machine-readable mirror in JSON mode (future): `StelResponse { envelope, body }`.

**Trust axiom:** `session_net_vs_manual` in envelope must match L4 aggregate within ±1% on same session.

---

## `StelLedgerEvent` (L4 append-only)

```json
{
  "ts_ms": 1710000000000,
  "plan_id": "…",
  "surface": "symforge",
  "intent": "trace",
  "decision": "serve",
  "tools_called": ["find_references"],
  "predicted_response_tokens": 400,
  "actual_response_tokens": 420,
  "manual_baseline_tokens": 800,
  "net_vs_manual": 380,
  "equivalence": null,
  "route_confidence": "exact"
}
```

Battery runs attach `equivalence` from sf-bench judge post-hoc.

---

## `CalibrationState` (L4 → L2 feedback)

Per `(tool, intent_bucket)` EMA:

```json
{
  "tool": "get_file_context",
  "intent": "read",
  "ema_predict_error": 0.08,
  "sample_count": 240,
  "fudge_multiplier": 1.05
}
```

`fudge_multiplier` adjusts L2 `est_response_tokens`. Updated after each battery run + production samples (when analytics enabled).

**Self-correction:** if `ema_predict_error > 0.20` after 50 samples → controller tightens caps automatically (+5% safety margin) until error drops.

---

## Path test schema (`routes.golden.jsonl`)

One JSON object per line — **path test** corpus (not unit test).

```json
{
  "id": "tokio-trace-hard_link",
  "request": { "query": "who calls hard_link", "intent": "auto" },
  "index": { "repo": "tokio", "sha": "da044f27…" },
  "must_call": ["find_references"],
  "must_not_call": ["get_file_content"],
  "expected_decision": "serve",
  "max_response_tokens": 1000,
  "notes": "sf-bench T2 variant"
}
```

Replay: spawn MCP with pinned index → call `symforge` → compare actual plan/decision/tools to golden.

---

## Performance test schema (`results.json` row extension)

Extend sf-bench rows with STEL fields:

```json
{
  "task": "T1",
  "tokens": { "S": 352, "M": 320, "N": 341 },
  "stel": {
    "plan_id": "…",
    "decision": "serve",
    "tools_called": ["get_symbol"],
    "predicted_tokens": 340,
    "actual_tokens": 352,
    "net_vs_manual": -32,
    "route_confidence": "inferred"
  }
}
```

`compare-results.js` gates H3–H8 on `stel.*` + existing equivalence.

---

## Controller algorithm (L2 normative)

```text
INPUT:  StelPlan, SessionContext, IndexSnapshot, CalibrationState, SurfaceProfile

1. CACHE: if target already in session → cache_hit
2. ESTIMATE: sum step est_response + invoke + schema_amortized
3. MANUAL: sum competent_manual_baseline(index_refs) per step
4. NET: manual - estimate - safety_margin(calibration)
5. if NET <= 0 → bypass (compute cheapest host action)
6. if NET <= margin_low → degrade (tighten max_tokens, sections, limits)
7. if plan.confidence == fallback && NET < margin_high → degrade mandatory
8. else → serve

OUTPUT: StelDecision
```

Language-agnostic: steps 2–3 use **bytes/lines only** from index.

---

## Dynamic surface (L0 registry)

| Profile | `tools/list` | When |
|---------|--------------|------|
| `compact` | 3 STEL tools | default v8 |
| `full` | 32 legacy | migration, sf-bench A/B |
| `expanded` | compact + on-demand core subset | after `listChanged` + host support |

Expansion triggered only when trajectory replay proves a core tool beats facade chain on pinned rows.

---

## Implementation order (schema-first)

| Step | Deliverable | Proof |
|------|-------------|-------|
| **S1** | This document + `routes.golden.jsonl` seed (36 sf-bench rows; include `expected_equiv`, `expected_decision`) | review |
| **S2** | Rust types in `src/stel/mod.rs` matching schemas | compile |
| **S3** | Compact surface (`SYMFORGE_SURFACE=compact`) — 3 tools in `tools/list` | **H1**; external **H5** (one MCP call) once L1–L2 execute inside that call (full H5 proof: Phase 2 exit per gap plan §7) |
| **S4** | `StelRequest` MCP tool + envelope formatter | path replay 5 rows |
| **S5** | L1 plan builder (extend smart_query) | H2 partial |
| **S6** | L2 controller + bypass | **H3/H4** on **compact** surface |
| **S7** | L4 ledger + calibration | H7 repeatability |

**Order rationale (post–adversarial review):** compact **H1 before controller** so Phase 2 battery runs on real economics, not 62 kB schema tax.

**Nothing in S4–S7 merges without v8 battery diff PASS vs v8 baseline (post-8.0).**

---

## Axe list (7.x patterns superseded by STEL)

| Inferior (axe) | Superior (survives) | Proof |
|----------------|---------------------|-------|
| 32-tool eager `tools/list` | compact 3-tool surface | H1 schema bytes |
| Single-hop `ask` without economics | `symforge` + L2 controller | H3/H4 |
| Health self-report as savings headline | envelope + sf-bench | diff vs M |
| Re-fetch without session check | cache_hit decision | path golden rows |
| Whole-file baseline in product totals | windowed manual baseline | sf-bench spec |
| Unbounded outline on large files | degrade + max_tokens | T3 large rows |

If STEL component fails its gate → **axe the component**, not the gate.
