# Phase 1 STEL checkpoint (compact-3 surface truthful)

**Branch:** `v8/stel-architecture`  
**Checkpoint commit:** `df89308` — *Add Phase 1 guarded symforge_edit apply for single-symbol replace*  
**Status:** Phase 1 **L1–L4 on compact `symforge` and preview-and-apply `symforge_edit`**, plus compact `status` and observational calibration. **Compact-3 is fully truthful at the MCP surface level** — all three advertised tools have real handlers.

This document captures implementation state after the guarded `symforge_edit` apply slice (`df89308`). It does **not** change runtime behavior.

---

## Evidence anchors

| Anchor | Commit / artifact | Role |
|--------|-------------------|------|
| Phase 0 evidence bundle | `08f7d14` | §12A measurement artifacts (A-019 bundle `f26f28b`; remediation `e9f4102` / `c3581a5`) |
| Independent GO / signoff | `07b42a8` | *Record Phase 0 12A independent GO decision* — authorization to implement `src/stel/` |
| Phase 1 golden replay closure | `b2c0d6a` | One-step golden planner mismatches closed (29 serve rows) |
| Preview-only edit checkpoint | `cabd978` | Preview-only `symforge_edit` handler (dry_run default) |
| Guarded apply semantics spec | `56072ce` | Normative guarded apply contract in [`stel-schema.md`](stel-schema.md) |
| Phase 1 tip (this checkpoint) | `df89308` | Guarded `symforge_edit` apply (`apply: true`, single-file single-symbol) |

**Deferred (not blocking this checkpoint):** `B-RESULTS` — RESULTS.md §8.7 post-8.0 baseline only.

---

## Phase 1 implementation commits

| Commit | Slice | Summary |
|--------|-------|---------|
| `d145699` | **S2** | Schema scaffolding — `src/stel/types.rs`, envelope, compact surface registry |
| `0f4c3d9` | **S3** | Compact `tools/list` — production list from `stel::compact_surface_tools()` when `SYMFORGE_SURFACE=compact` |
| `b3ee2a2` | **S4** | Compact `symforge` handler — trust envelope + legacy tool dispatch path |
| `e69b732` | **S4 exit** | Golden replay validation — five ask/planner-aligned rows in `tests/stel_golden_replay.rs` |
| `62d6bfd` | **L1** | Planner — `StelRequest` → single-step `StelPlan` |
| `20b4e17` | **L2** | Economics controller — `evaluate_plan` → `StelDecision` / `StelEstimate` metadata |
| `5038ac3` | **L3** | P-FF bypass enforcement — skip legacy dispatch when `StelBypassBody` present |
| `31d9bf1` | **L4** | Session ledger — in-memory `StelLedgerEvent` + envelope `ledger:` JSON line |
| `3995643` | **Status** | Compact `status` handler — operational STEL/index headline |
| `15c0685` | **Calibration** | Observational calibration summary from in-memory ledger (read-only) |
| `24e1b7c` | **Golden** | Full golden corpus classification (serve / P-FF / multi-hop / mismatch partitions) |
| `b2c0d6a` | **L1** | Golden one-step planner mismatch reduction — 29 supported serve rows |
| `cabd978` | **Edit** | Preview-only `symforge_edit` — dry-run `replace_symbol_body`, no apply path |
| `9f6a86c` | **Docs** | Checkpoint doc for preview-only `symforge_edit` |
| `56072ce` | **Docs** | Normative guarded apply semantics in `stel-schema.md` |
| `df89308` | **Edit** | Guarded `symforge_edit` apply — `apply: true` single-file single-symbol `replace_symbol_body` |

Prior: `07b42a8` Phase 0 GO · `08f7d14` evidence anchor (pre-implementation).

---

## What is complete

### S2 — Schema scaffolding

- Wire types in [`stel-schema.md`](stel-schema.md): `StelRequest`, `StelPlan`, `StelDecision`, `StelLedgerEvent`, `StelEditRequest`, etc.
- `src/stel/envelope.rs` — `StelTrustEnvelope` text formatter
- `src/stel/surface.rs` + `surface_list.rs` — compact-3 registry

### S3 — Compact tools/list

- `SYMFORGE_SURFACE=compact` → three tools: `symforge`, `symforge_edit`, `status`
- Schema bytes validated under H1 budget (A-019 compact-3 winner)
- Phase 0 [`surface_probe.rs`](../src/protocol/surface_probe.rs) **frozen** for A-005/A-019 measurement

### S4 — Compact symforge handler + golden replay

- `symforge_facade_tool` → L1→L2→L3 path when compact (probe relay unchanged for Phase 0 harness)
- Golden corpus: **29 supported serve** + **4 P-FF bypass** rows replayed in `tests/stel_golden_replay.rs`
- S4 minimum subset (`S4_EXIT_ROW_IDS`, five rows) remains a named floor inside supported serve replay

### L1 — Planner (read path)

- [`src/stel/planner.rs`](../src/stel/planner.rs) — `build_plan()`: intent buckets, query patterns, `smart_query` fallback
- Single-step plans only (multi-hop golden rows deferred)

### L1 — Edit planner (preview and apply paths)

- [`src/stel/edit_planner.rs`](../src/stel/edit_planner.rs) — `build_edit_plan()`: validates path/symbol/body, emits single-step `replace_symbol_body`
- **Default:** `dry_run: true` when `apply` is omitted or `false`
- **`apply: true`:** `dry_run: false` in plan args; explicit opt-in only
- Rejects unsafe paths (`..`, absolute paths) and missing symbol/body before planning
- Forwards optional `idempotency_key` into `replace_symbol_body` args

### L1 — Guarded apply pre-flight

- [`src/stel/edit_apply.rs`](../src/stel/edit_apply.rs) — `run_pre_apply_gates()` for `apply: true` only
- **Safety gates:** index readiness, symbol resolution, on-disk vs index byte match, `if_match` body check
- **Idempotency / already-applied:** when body already matches requested content and no `idempotency_key`, returns success without rewrite; when `idempotency_key` is set, defers to `replace_symbol_body` idempotency replay (no double write)
- **Scope:** single file, single symbol, `replace_symbol_body` only — no multi-file edits

### L2 — Economics metadata

- [`src/stel/controller.rs`](../src/stel/controller.rs) — conservative schema (45) + invoke (80) per call (A-006 path)
- P-FF detection → `bypass` + `StelBypassBody` (read path only)
- `evaluate_edit_plan()` for structural edits (no NL P-FF bypass)
- Serve when predicted net > margin; preview via `StelEstimate` on `symforge`
- **No calibration-driven fudge or margin changes yet**

### L3 — P-FF bypass enforcement (read path)

- [`src/stel/executor.rs`](../src/stel/executor.rs) — `is_enforced_bypass()` gates legacy dispatch
- Bypass response: trust envelope + host-read instruction (no `Chosen tool:` line)
- Non-P-FF negative-net bypass metadata **not** enforced yet (still serves)

### L3 — Edit preview and guarded apply

- `symforge_edit_stel_handler` dispatches `replace_symbol_body` with `dry_run: true` by default (preview)
- **`apply: true`** runs pre-apply gates, then dispatches `replace_symbol_body` with `dry_run: false`
- Apply response includes changed file path, byte range, line range, write mode (`committed` / `already_applied`)
- Trust envelope + ledger on both preview and apply success paths
- Pre-apply validation failures return `InvalidRequest` **without** envelope
- **No multi-hop routing** — always single-step `replace_symbol_body`

### L4 — Session ledger

- [`src/stel/ledger.rs`](../src/stel/ledger.rs) — `SessionLedger` on `SymForgeServer` (in-memory, no persistence)
- Records: plan id, route tool, decision, bypass flag, schema/invoke tokens, predicted net, legacy executed, output bytes/tokens
- Compact `ledger: {…}` JSON embedded in trust envelope
- Edit calls record `surface: "symforge_edit"` in ledger events
- `legacy_executed: true` only when apply commits via atomic write + reindex
- `symforge` preview path (`preview: true`) does not append ledger rows

### Compact `status` handler

- [`src/stel/status.rs`](../src/stel/status.rs) — `status_stel_tool` when `SYMFORGE_SURFACE=compact`
- `detail: compact` (default) — operational headline: surface, Phase 0 anchors, L1–L4 availability, handler state, ledger event count, index readiness
- Reports **`handler_symforge_edit: preview-and-apply`** (not preview-only)
- `detail: full` — adds project, symbol count, session tokens, last ledger decision/route, and calibration section

### Observational calibration (read-only)

- [`src/stel/calibration.rs`](../src/stel/calibration.rs) — `summarize_calibration()` over in-memory `SessionLedger` events
- **Derived only** from appended `StelLedgerEvent` rows; does not write back to L2 or alter serve/bypass decisions
- Exposed in `status detail: full` under `── calibration (observational) ──`
- **No persistence** across restarts; **no auto-tuning**; **no L2 margin or route decision changes**

---

## Runtime flow (compact `symforge`)

```mermaid
flowchart TD
  L0["L0 symforge MCP call"] --> L1["L1 planner.build_plan"]
  L1 --> L2["L2 controller.evaluate_plan"]
  L2 --> P{preview?}
  P -->|yes| EST["StelEstimate JSON"]
  P -->|no| B{P-FF bypass?}
  B -->|yes| BYP["L3 format_bypass_body\nno legacy tool"]
  B -->|no| SER["L3 dispatch planned tool"]
  BYP --> L4["L4 capture_ledger + envelope"]
  SER --> L4
  EST --> ENV["Trust envelope only"]
  L4 --> CAL["status detail:full\nreads ledger → calibration summary"]
```

## Runtime flow (compact `symforge_edit` — preview and guarded apply)

```mermaid
flowchart TD
  E0["L0 symforge_edit MCP call"] --> EV["L1 edit_planner.validate"]
  EV -->|invalid| REJ["InvalidRequest\nno envelope"]
  EV --> A{apply: true?}
  A -->|no| EL2P["L2 evaluate_edit_plan"]
  EL2P --> EDP["L3 replace_symbol_body\ndry_run=true"]
  EDP --> EL4P["L4 envelope + ledger\nlegacy_executed=false"]
  A -->|yes| G["edit_apply.run_pre_apply_gates"]
  G -->|reject| REJ
  G -->|already applied| EL4A["L4 envelope + ledger\nno rewrite"]
  G -->|ready| EL2A["L2 evaluate_edit_plan"]
  EL2A --> EDA["L3 replace_symbol_body\ndry_run=false"]
  EDA --> EL4C["L4 envelope + ledger\nbyte/line metadata\nlegacy_executed on commit"]
```

**Defaults:** preview/dry_run when `apply` is omitted or `false`. **`apply: true` is explicit opt-in only.**

---

## L0 surface choice (unchanged)

**Compact-3** remains the selected Phase 1 L0 surface ([A-019](research/A-019-l0-surface-choice.md) **VALIDATED**).

| Tool | Shipped handler | Notes |
|------|-----------------|-------|
| `symforge` | **Yes** — full L1–L4 path | Production compact read/explore facade |
| `status` | **Yes** — operational + calibration (full) | Requires `SYMFORGE_SURFACE=compact` |
| `symforge_edit` | **Yes** — preview-and-apply L1–L4 path | Default preview/dry_run; `apply: true` commits single-symbol `replace_symbol_body` |

---

## Test coverage at checkpoint

| Suite | What it proves |
|-------|----------------|
| `cargo test stel::` | Unit tests across types, planner, edit_planner, edit_apply, controller, executor, ledger, calibration, status, envelope, golden_replay helpers |
| `tests/stel_golden_replay.rs` | Classifies all 36 golden rows; replays **29 supported serve** + **4 P-FF bypass** rows |
| `tests/stel_symforge_edit.rs` | Preview/apply separation, unsafe path rejection, missing symbol, `if_match` mismatch, already-applied idempotency, idempotency-key replay, successful single-symbol apply — **all use `tempfile::tempdir()` fixtures only; no real repo source writes** |
| `tests/stel_l3_enforcement.rs` | P-FF bypass skips legacy tools; serve still executes |
| `tests/stel_l4_ledger.rs` | Serve and P-FF rows produce envelope `ledger:` + session ledger events |
| `tests/stel_status.rs` | Compact guard, `handler_symforge_edit: preview-and-apply`, full detail + calibration after serve |
| `cargo test --lib protocol::surface_probe` | Phase 0 measurement schemas unchanged |

Golden corpus has **36 rows** partitioned by `classify_golden_corpus()`:

| Category | Count | Notes |
|----------|-------|-------|
| Supported serve replay | 29 | L1 planner matches `must_call[0]`; trust envelope + `ledger:` validated |
| Supported P-FF bypass replay | 4 | L3 enforced bypass; no legacy tool execution |
| Deferred multi-hop | 3 | `DEFERRED_MULTI_HOP_ROW_IDS` — planner multi-step not shipped |
| Deferred planner mismatch | 0 | Narrow L1 route patterns cover remaining single-hop rows |

---

## Preserved / unchanged

- Phase 0 **`surface_probe`** and `_probe_*` harness relay on `symforge`
- Compact-3 `tools/list` production path vs frozen probe schemas (A-025 `symforge_edit` schema within budget)
- `symforge` serve/bypass execution semantics
- L2 margins and calibration behavior (observational only)
- Full 32-tool surface when `SYMFORGE_SURFACE=full`

---

## Explicitly out of scope at this checkpoint

| Item | Status |
|------|--------|
| Multi-step planner / executor chains | L1 single-step only; **3 golden multi-hop rows deferred** |
| Multi-file `symforge_edit` apply | Not implemented — single file + single symbol only |
| Calibration auto-tuning (`CalibrationState` fudge → L2) | Not implemented — observational summary only |
| Calibration / ledger persistence | In-memory only |
| H3–H8 battery gates on compact surface | Not claimed |
| `B-RESULTS` / RESULTS.md §8.7 | Deferred post-8.0 |

---

## Guarded apply semantics (implemented — `df89308`)

**Normative contract:** [`stel-schema.md`](stel-schema.md) — sections **`StelEditRequest`** and **`Guarded apply semantics`**.

Shipped behavior (`df89308`):

| Requirement | Status |
|-------------|--------|
| `apply: true` explicit opt-in; default preview/dry_run | **Shipped** |
| No silent writes | **Shipped** |
| Path/symbol/body validation; traversal and absolute-path rejection | **Shipped** |
| Pre-apply symbol resolution and on-disk content verification | **Shipped** |
| `if_match` mismatch rejection | **Shipped** |
| Idempotency-key replay and already-applied handling | **Shipped** |
| Trust envelope + ledger on apply | **Shipped** |
| Changed file + byte/line range reporting | **Shipped** |
| `status` reports `handler_symforge_edit: preview-and-apply` | **Shipped** |
| Single-file / single-symbol scope | **Shipped** |
| Multi-file edits | **Out of scope** |

---

## Suggested next boundaries (risk order)

1. **Multi-hop routing** — replay the three `chain: multi` golden rows; larger planner + runtime expansion (optional — strong Phase 1 milestone already reached without this)
2. **Calibration persistence** — durable ledger + optional auto-tuning gate (still no silent L2 changes)
3. **`B-RESULTS` / §8.7** — operator-triggered after 8.0 tag baseline exists

---

## Source map

| Module | Layer |
|--------|-------|
| `src/stel/types.rs` | Wire types (`StelEditRequest`: `apply`, `if_match`, `idempotency_key`) |
| `src/stel/surface.rs`, `surface_list.rs` | L0 registry |
| `src/stel/planner.rs` | L1 read routing |
| `src/stel/edit_planner.rs` | L1 edit routing (preview + apply plan) |
| `src/stel/edit_apply.rs` | Guarded apply pre-flight gates |
| `src/stel/controller.rs` | L2 |
| `src/stel/executor.rs` | L3 bypass enforcement |
| `src/stel/ledger.rs` | L4 record |
| `src/stel/calibration.rs` | Observational calibration summary |
| `src/stel/status.rs` | Compact `status` handler |
| `src/stel/handler.rs`, `envelope.rs` | Envelope + preview |
| `src/stel/golden_replay.rs` | Golden validation helpers |
| `src/protocol/tools.rs` | `symforge_stel_handler`, `symforge_edit_stel_handler`, `status_stel_tool` |
| `src/protocol/surface_probe.rs` | Phase 0 frozen measurement |

---

## Related docs

- [stel-schema.md](stel-schema.md) — normative types, controller algorithm, **guarded apply semantics**
- [stel-architecture.md](stel-architecture.md) — charter and H1–H8 gates
- [v8-gap-closure-plan.md](v8-gap-closure-plan.md) — binding pre-flight and phase map
- [phase0-12a-review-signoff.md](research/phase0-12a-review-signoff.md) — GO decision record
