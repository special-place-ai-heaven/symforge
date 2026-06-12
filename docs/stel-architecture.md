# STEL — SymForge Tool Execution Layers (v8.0.0 target)

Branch: `v8/stel-architecture`  
Current release: `7.21.1`  
Target release: **`8.0.0`** (major — breaking MCP public surface)

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

```text
L0  SURFACE     symforge | symforge_edit | symforge_status
L1  ROUTER      QueryIntent → execution plan (extend smart_query)
L2  CONTROLLER  SavingsPredictor: serve | degrade | bypass | cache
L3  CORE        Existing 32 tool handlers (internal in compact mode)
L4  DATA        Index, SessionContext, TokenStats, Analytics
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

Built on `src/protocol/smart_query.rs` (`QueryIntent`, 39+ unit tests).

Extensions for v8:

- Output **execution plan** (multi-step internal chain), not only single tool
- Golden-route corpus: JSON fixtures + sf-bench task mapping
- Trust envelope on every L0 response (extends existing `ask` envelope)

### L2 — Controller

New module (`src/savings/` or `src/protocol/savings.rs`):

- Inputs: index metadata, `SessionContext`, session schema amortization, route confidence
- Decisions: **Serve**, **Degrade**, **Bypass**, **CacheHit**, **Chain**
- Baseline: `competent_manual_baseline_chars`, `resolve_read_max_tokens` (from savings baseline work)

Analytics extension per event:

- `predicted_tokens`, `actual_tokens`, `decision`, `intent`, `net_vs_manual`

### L3 — Core

No removal of handlers. Compact mode: omit from `tools/list`; invoke from L0/L1 only.

Carry forward from `fix/honest-token-savings-baseline` (uncommitted on branch start):

- Content-anchored symbol fallback, small-file outline-only, default read budgets, search_text merge

### L4 — Data

Existing: `SessionContext`, `TokenStats`, `AnalyticsObservation`, live index.

Self-correction loop:

1. Golden routes CI (L1)
2. sf-bench regression CI (equivalence + tokens)
3. Prediction EMA calibration (L2 → analytics)
4. Drift guard: catalog ⊆ registered tools (existing `init.rs` test)

## Release gates (all required for 8.0.0)

| ID | Gate | Metric |
|----|------|--------|
| **H1** | Compact schema | `tools/list` JSON ≤ **5,000 B** (vs 62,397 today) |
| **H2** | Route accuracy | Golden corpus ≥ **95%** intent + primary tool match |
| **H3** | Small-file economics | sf-bench `*_small` tasks: **0** rows with `sGteM` when bypass enabled |
| **H4** | Session net | Full battery session net vs manual ≥ **0** with controller |
| **H5** | Round-trips | Compact mode completes sf-bench tasks in **1** MCP call where plan allows |
| **H6** | Equivalence | EQUIVALENT rate ≥ **50%** (36-task corpus), no regression vs 7.21.1 baseline on wins |
| **H7** | Conformance | Existing MCP conformance + `cargo test` green |

## Implementation phases

### Phase 0 — Baseline (merge prerequisite)

- [ ] Land windowed savings baseline + payload fixes from savings branch work
- [ ] sf-bench re-run on release binary; pin `results-7.21.1-baseline.json`
- [ ] Conformance test: measure `tools/list` schema bytes

### Phase 1 — Compact surface

- [ ] `SYMFORGE_SURFACE` profile filtering in `list_tools`
- [ ] `symforge` facade (extend `ask`); deprecate direct `ask` in full profile
- [ ] MCP `listChanged` capability (optional tool expansion)
- [ ] Update `SYMFORGE_TOOL_NAMES` / client init allowlists for compact trio

### Phase 2 — SavingsController

- [ ] `SavingsPredictor` + unit tests against sf-bench rows
- [ ] Bypass/degrade/cache wired into top handlers
- [ ] Analytics fields for calibration

### Phase 3 — Internal chaining

- [ ] Plan executor inside `symforge` (search → symbol, sparse → text)
- [ ] Single trust envelope for multi-step execution

### Phase 4 — Self-correction + release

- [ ] CI: golden routes + sf-bench gates (H1–H7)
- [ ] `symforge_status` aggregates net savings + calibration
- [ ] CHANGELOG 8.0.0 migration guide (compact default, renamed tools)
- [ ] Bump `Cargo.toml` to `8.0.0` only when all gates pass

## Migration (7.x → 8.0)

For hosts that hard-code tool names:

1. Replace `mcp__symforge__ask` → `mcp__symforge__symforge`
2. Set `SYMFORGE_SURFACE=full` to retain 32-tool surface during transition
3. Read bypass hints literally (`BYPASS: use Read on …`) — do not retry SymForge on same target

## References

- sf-bench spec: `SYMFORGE_TOKEN_SAVINGS_BENCHMARK_SPEC.md`
- Router: `src/protocol/smart_query.rs`
- Trust envelope: `ask` handler in `src/protocol/tools.rs`
- External: MCP `tools/list_changed` ([spec](https://modelcontextprotocol.io/specification/2025-11-25/server/tools)), Anthropic Tool Search (3–5 loaded tools)
