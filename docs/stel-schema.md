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

**Single L0 entry for structural edits:** `symforge_edit` runs L1→L2→L3→L4 in one MCP round-trip. **Phase 1 ships preview-only** (`dry_run`); guarded apply is specified below but **not implemented**.

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

## `StelEditRequest` (L0 → L1 edit surface)

MCP input schema for `symforge_edit` (JSON Schema source of truth for compact surface; A-025 budget).

### Shipped wire schema (Phase 1 — preview-only)

```json
{
  "type": "object",
  "properties": {
    "path": { "type": "string", "description": "Repository-relative file path" },
    "symbol": { "type": "string", "description": "Symbol name to replace" },
    "body": { "type": "string", "description": "Complete new symbol source" },
    "intent": { "type": "string", "enum": ["edit"] }
  },
  "required": ["path"]
}
```

| Field | Role |
|-------|------|
| `path` | Required; repository-relative path to the file containing the symbol |
| `symbol` | Required for preview/apply; names the symbol whose body will be replaced |
| `body` | Required for preview/apply; full replacement source for the symbol definition |
| `intent` | Optional; `edit` when set (L0 routes structural mutations to `symforge_edit`, not `symforge`) |

**Runtime today (commit `cabd978`):** every call is **preview-only**. The handler plans `replace_symbol_body` with `dry_run: true`, dispatches the legacy tool in dry-run mode, prepends the trust envelope, and appends a ledger row with `surface: "symforge_edit"`. **No file bytes are written.**

### Future wire field — `apply` (not shipped)

When guarded apply is implemented, extend the schema with an explicit opt-in flag (subject to A-025 byte budget or documented pivot):

```json
{
  "apply": {
    "type": "boolean",
    "default": false,
    "description": "If true, commit the planned edit after pre-apply validation. Default false = preview/dry_run only."
  }
}
```

| Rule | Norm |
|------|------|
| Default | `apply` omitted or `false` → **preview/dry_run only** (current behavior) |
| Opt-in | `apply: true` → may commit **only** after all guarded-apply gates pass |
| Forbidden | Implicit apply, env-var apply, or “second call auto-commits preview” patterns |

Optional future companion fields (also not shipped): `idempotency_key` (replay committed apply), `symbol_line` (disambiguation), `expected_body_hash` or `if_match` (optimistic concurrency). Any addition must stay within A-025 or trigger the G-025 pivot documented in [`v8-gap-closure-plan.md`](v8-gap-closure-plan.md).

---

## Guarded apply semantics (normative — not implemented)

**Gate:** no `apply: true` implementation merges until this section is satisfied by code **and** tests. Phase 1 checkpoint: [`phase1-stel-checkpoint.md`](phase1-stel-checkpoint.md) (`9f6a86c`).

### Invariants

1. **Preview remains the default** — absent `apply: true`, the handler must call legacy edit tools with `dry_run: true` only.
2. **No silent writes** — a successful MCP response must never mutate on-disk source unless the request explicitly opted into apply and passed every gate below.
3. **Single-hop, single-file** — Phase 1 guarded apply is one `replace_symbol_body` on one `path` + one `symbol`. Multi-file edits (`batch_edit`, `batch_rename`, etc.) are **out of scope** unless separately specified.
4. **Same trust path as read** — apply responses must include `StelTrustEnvelope` + compact `ledger:` metadata, identical contract to `symforge` serve.

### L1 validation (preview and apply)

These checks run **before** planning; failures return `reject` / `InvalidRequest` with **no** envelope and **no** ledger row:

| Check | Rule |
|-------|------|
| Path present | `path` non-empty after trim |
| Path traversal | Reject `..` segments |
| Absolute paths | Reject leading `/` or `\\` |
| Drive / scheme prefixes | Reject `:` in path (Windows drives, URLs) |
| Symbol / body | Both required for edit preview and apply |
| Index ready | Index must be loaded for the repository root |

Implemented today in [`edit_planner.rs`](../src/stel/edit_planner.rs); apply must not weaken these rules.

### Pre-apply gates (`apply: true` only)

Additional gates before any write:

| Gate | Requirement |
|------|-------------|
| Symbol resolution | Index resolves exactly one symbol span for `(path, symbol)` (or explicit `symbol_line` when added); ambiguous or missing → reject, no write |
| Edit-safety tier | Target file/language must pass the same structural-edit safety checks as legacy `replace_symbol_body` |
| Content verification | Before write, verify on-disk bytes for the resolved symbol span match the index snapshot used for planning (detect external drift). Mismatch → reject, no write |
| Expected match (recommended) | Caller may supply `if_match` / hash of the **current** symbol body; mismatch → reject, no write |
| Idempotency | When `idempotency_key` is set, replay with the same key + same canonical request hash returns the stored outcome; same key + different hash → deterministic reject |
| Already applied | If the on-disk symbol body already equals the requested `body`, return success without rewrite (idempotent no-op) or return a deterministic “already applied” outcome — must not double-apply destructively |

### Apply execution

```text
INPUT:  StelEditRequest with apply=true, StelPlan (replace_symbol_body), IndexSnapshot

1. Run all L1 validation gates
2. Run all pre-apply gates (symbol resolve, safety tier, content verify)
3. Re-evaluate L2 economics (evaluate_edit_plan) — unchanged margins vs preview slice
4. Dispatch replace_symbol_body with dry_run=false
5. On success: re-index affected file; capture ledger with legacy_executed=true
6. On failure: no partial multi-file state (single-file scope); return error with envelope when execution started

OUTPUT: StelTrustEnvelope + body + ledger line
```

**Rollback / error behavior:** Phase 1 apply is **single-symbol, single-file**. On failure after write begins, behavior follows legacy `replace_symbol_body` atomicity (no multi-file transaction). Callers must use `what_changed` / `validate_file_syntax` after apply. Multi-file rollback is out of scope.

### Response body (apply)

Apply responses must report, at minimum:

| Field | Content |
|-------|---------|
| Changed files | Repository-relative path(s) — one file in Phase 1 |
| Symbol | Name (and `symbol_line` when used) |
| Byte range | Start/end byte offsets of the replaced span (from index resolution) |
| Line range | Start/end 1-based lines covering the replaced definition |
| Write mode | `committed` vs `dry_run` |
| Tool | `replace_symbol_body` |

Dry-run preview responses continue to include `[DRY RUN]` and `Write semantics: dry run (no writes)` from the legacy tool.

### Ledger (apply)

`StelLedgerEvent` for apply must set:

- `surface`: `"symforge_edit"`
- `decision`: `serve` when commit succeeds (or `reject` when gated — see preview invariant: no ledger on pre-plan validation failures)
- `tools_called`: `["replace_symbol_body"]` when legacy tool ran
- `legacy_executed`: `true` only when `dry_run=false` and bytes were committed

Envelope `ledger:` JSON must mirror the same metadata as preview (`plan_id`, `route_tool`, `decision`, `legacy_executed`, token economics).

### `status` reporting

| Phase | `handler_symforge_edit` value |
|-------|-------------------------------|
| Preview-only (today) | `preview-only` |
| After guarded apply ships | `active` or `preview-and-apply` — must **not** imply apply is available while default remains dry-run |

`status` must never report apply-enabled until integration tests prove opt-in writes.

### Normative test matrix (future apply slice)

Tests must exist before apply ships (extend [`tests/stel_symforge_edit.rs`](../tests/stel_symforge_edit.rs)):

| Test | Proves |
|------|--------|
| Default / omitted `apply` | No bytes written; `dry_run` invoked |
| Explicit `apply: false` | Same as default |
| Preview then apply separation | Preview call leaves file unchanged; apply call with same args writes once |
| Unsafe path / missing fields | Rejected without write (existing preview tests) |
| Symbol not found | Rejected without write |
| Content drift / `if_match` mismatch | Rejected without write when on-disk span ≠ expected |
| Idempotent replay | Same `idempotency_key` + request → same outcome, no double write |
| Already applied | Second apply with same body → no destructive rewrite |
| Successful apply | Bytes change; envelope + `ledger:` present; `legacy_executed: true` |
| Ledger surface | `surface: "symforge_edit"` on apply rows |

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

Prepended to every `symforge` and `symforge_edit` body (text or structured). Parsed by hosts; displayed to LLM.

**Default is the COMPACT one-liner.** With no env opt-in, every live MCP call
prepends a single honest line — route, admission decision, and the `(est.)`
served-token figure — to keep per-call context noise minimal:

```text
── stel · trace → find_references (exact) · serve · ~420 tok served (est.) ──
```

Set `SYMFORGE_STEL_FULL=1` to restore the full multi-line economics block (the
on-request / contract form below). That is the only variable that changes this
behavior. `SYMFORGE_STEL_COMPACT` is now a no-op — compact is already the
default, so the variable is neither read nor required. The full block stays the
normative contract surface (it is what the golden-replay validators and the
honesty regression assert):

```text
── stel ──
plan: trace → find_references (exact)
decision: serve
tokens: ~420 served (est. chars/4) · est. 380 fewer vs manual (heuristic) · schema 45 · invoke 80
predicted: ~400 (heuristic) · error: 5.0%
session_tokens_served: 1240
calibration: deferred
──
```

The compact one-liner carries the SAME honesty load it always has: the served
figure stays `(est.)`-qualified and no measured-saving claim appears; it simply
drops the per-call economics detail. All token figures are estimates, never
measured: `served`/`predicted` are
`chars/4` approximations; the "fewer vs manual" figure is a heuristic
prediction from the planner `400/800` constants (010 honesty contract,
FR-001). `session_tokens_served` is a monotonic gross running total of tokens
served this session — NOT a net of savings, so it carries no `+` sign and can
only grow (FR-002, TR-05/TR-11). On a `reject` the per-call comparison reads
`n/a (rejected)`. `calibration: deferred` because the auto-tuning seam
(`CalibrationState`) is inert (N-1).

Machine-readable mirror in JSON mode (future): `StelResponse { envelope, body }`.

**Trust axiom (target, A-015 OPEN):** `session_tokens_served` is a gross
counter today; matching it to an L4 net aggregate within ±1% remains an OPEN
assumption (A-015), not a shipped guarantee.

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

Edit apply rows use `"surface": "symforge_edit"` with the same field contract.

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
| **S4e** | `symforge_edit` preview-only handler | `tests/stel_symforge_edit.rs` |
| **S4e+** | `symforge_edit` guarded apply (`apply: true`) | normative tests in `stel-schema.md` § Guarded apply — **blocked until S4e+ tests pass** |
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
