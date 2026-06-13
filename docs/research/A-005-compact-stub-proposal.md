# A-005 — Smallest non-shipping compact surface measurement stub (proposal)

**Status:** **PROPOSAL ONLY — awaiting approval before any source edit**  
**Blocker:** B-A005  
**Date:** 2026-06-13

## Problem

- `scripts/measure-schema-bytes.ps1` sets `SYMFORGE_SURFACE=compact` but **no Rust code reads that env var** (SymForge search: zero matches in `src/`).
- Current `tools/list` always returns **32 legacy tools** via `ToolRouter::list_all()`.
- Last run: MCP probe **PARTIAL** — stderr tracing polluted node capture; compact bytes never measured.
- §12A allows a **non-shipping stub** in symforge (not `src/stel/`) per gap plan §12A surface-choice note.

## Goal (measurement only)

Produce a real `tools/list` JSON payload for:

| Profile | Env value | Expected tools | Budget |
|---------|-----------|----------------|--------|
| `full` | `full` (default) | 32 legacy | informational |
| `compact` | `compact` | 3 STEL names | **≤ 5,000 B** (A-005 / H1) |
| `edit` slice | (within compact list) | `symforge_edit` schema | **≤ 1,500 B** (A-025) |

Tool names per [`stel-schema.md`](../stel-schema.md) L0 registry: **`symforge`**, **`symforge_edit`**, **`status`**.

`call_tool` for compact stub tools may return deterministic errors — **only `tools/list` byte size is required for Phase 0**.

---

## Recommended approach (smallest correct diff)

### 1. New module `src/protocol/surface_probe.rs` (~80–120 lines)

- `pub fn surface_profile() -> SurfaceProfile` — reads `SYMFORGE_SURFACE` (`full` | `compact` | `meta`; default `full`).
- `pub fn list_tools_for_profile(profile: SurfaceProfile) -> Vec<Tool>`:
  - **`full`:** delegate to existing `SymForgeServer::tool_router().list_all()` (unchanged behavior).
  - **`compact`:** return **3 static** `rmcp::model::Tool` values with JSON Schemas taken from [`stel-schema.md`](../stel-schema.md) `StelRequest` / edit schema / minimal status schema (draft-07, no `$ref` chains if possible).
- `pub fn compact_stub_call_tool(name: &str) -> Result<...>` — return `InvalidRequest` with message `"compact surface probe: not implemented"` (measurement-only).

**Does not touch `src/stel/**`.** Lives next to existing protocol surface.

### 2. Wire listing only (~15 lines)

In `src/protocol/mod.rs`:

- Change `tool_definitions()` to call `surface_probe::list_tools_for_profile(surface_profile())`.
- Override `list_tools` on `ServerHandler` (replace macro-generated default for that method only) to use the same filter.

**Do not** change `call_tool` routing for legacy 32 tools when `full` — only compact/meta paths short-circuit.

### 3. Fix harness script (~10 lines) — `scripts/measure-schema-bytes.ps1`

- Spawn symforge with `$env:RUST_LOG='off'` (and any symforge tracing env off).
- Capture **stdout only**; fail if JSON parse fails.
- Add `-Surface compact|full` flag; record per-tool byte breakdown optional.

### 4. Tests (~30 lines)

- `#[test] fn compact_surface_list_under_5000_bytes()` — uses `tool_definitions()` with env set in test.
- `#[test] fn full_surface_still_lists_32_tools()` — regression guard.

No integration with sf-bench required for A-005 stub landing.

---

## Alternatives considered (not recommended first)

| Option | Pros | Cons |
|--------|------|------|
| **Fixture JSON only** (`docs/fixtures/compact-tools-list.json`) | Zero Rust change | Not a live `tools/list`; contract wants measured bytes on real MCP response |
| **`symforge schema-bytes` CLI subcommand** | Isolated from MCP | New CLI surface; duplicates MCP serialization path |
| **Filter existing 32 tools down to 3 names** | Tiny diff | Schemas still legacy size → false FAIL on H1 |
| **Implement in `src/stel/surface.rs`** | Matches S3 long-term | **Violates constraint** — no `src/stel/` before GO |

---

## Out of scope for this stub (explicit)

- L1–L4 STEL execution
- Meta-tool surface (`meta-1` / `meta-2`) — separate A-019 battery work
- Changing H1 threshold
- Shipping compact as default without A-019 + full §12A green

---

## Approval gate

**Stop here.** No source changes until you approve:

- [ ] Approach: `surface_probe.rs` + `list_tools` override + script stderr fix
- [ ] Compact tool schemas: draft from `stel-schema.md` (may iterate if over 5kB)
- [ ] A-025 pivot unchanged if `symforge_edit` alone exceeds 1,500 B

After approval, implementation order:

1. Land stub + script fix
2. `cargo test` compact byte test
3. Re-run `measure-schema-bytes.ps1` → update `A-005-schema-bytes.json` + summary
4. Re-evaluate A-005/A-025 verdicts (still NO-GO if over budget without accepted pivot)
