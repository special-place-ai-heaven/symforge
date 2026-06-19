# SymForge 8.4.0 — compact-3 facade friction (AAP dogfood battery)

**Date**: 2026-06-19  
**Surface**: `symforge` + `symforge_edit` + `status` (compact-3)  
**Version tested**: 8.4.0  
**Source**: AAP agent onboarding battery (8 `symforge` + 1 `symforge_edit`); black-box findings validated against `src/stel/planner.rs`.

## What works well

- **find (symbol)** → `search_symbols` — exact match, helpful fallback docs.
- **trace (symbol)** → `find_references` — enclosing-symbol context, low prediction error.
- **symforge_edit preview** — path+symbol honored, exact `replace_symbol_body` dry-run, clear byte diff.
- **status** — `symforge_version`, index health, STEL wiring (version assertion TC lacks).
- **STEL per-call trace** — plan / route_tool / decision / tokens / ledger; self-explaining for diagnosis.

## Repro table (pre-fix)

All calls via compact `symforge` unless noted.

| # | intent / call | planner route (8.4.0) | result |
|---|---------------|----------------------|--------|
| A | auto · "Orient me: …main crates…" | find → search_files → search_text (prose tokenized) | 350 noise matches ❌ |
| B | find · `symbol=fail_and_cascade` | search_symbols | exact `dag.rs:255` ✅ |
| C | read · `symbol` + `path=dag.rs` | get_file_context | file outline, not body ❌ |
| D | orient · "map of workspace crates" | explore | Workspace* keyword hits, no map ❌ |
| E | read · `symbol`, no path | get_file_context `{path: query}` | File not found / reject ❌ |
| F | trace · `symbol=BlockedReason` | find_references `{name}` | 25 refs / 6 files ✅ |
| G | impact · `symbol=TaskStatus` | find_dependents `{path: query}` | no dependents for question ❌ |
| H | symforge_edit replace (dry-run) | replace_symbol_body `{dry_run:true}` | clean preview ✅ |

## Root cause

`symbol_lookup_step` was gated to **find/auto only**. Explicit `read` and `impact` buckets fell through to `route_read` / `route_impact`, which ignore `symbol` and shove the natural-language `query` into `path`. **Trace** and **symforge_edit** already consumed `symbol` — hence the asymmetry.

Orientation queries with no literal `"repo map"` routed to `explore` (keyword hits) or find-fusion (OR-literal explosion on auto).

## Fix (branch `fix/stel-symbol-aware-routing`)

1. Extend `symbol_lookup_step` to **read** intent → `get_symbol` (with or without `path`).
2. Add `symbol_impact_step` → `find_dependents` when `path` set; else `find_references` for symbol-level impact.
3. Add `orient_lookup_step` + `is_orient_query` → `get_repo_map` before find fusion; default `route_orient` to map.

## Cross-cutting (separate work)

- **Predictor calibration**: `durable_ledger: unavailable` on stdio MCP; errors wild on plan-only floors — documented/deferred (010).
- **max_tokens**: compact serve caps inject floors; find fusion can exceed budget while read truncates — inconsistent enforcement.

## AAP coordination

`stel` is `#[cfg(feature = "server")]` — planner changes do not affect AAP embed builds. AAP path-deps `../symforge` and can bump `Cargo.lock` to whatever version it needs after symforge releases.
