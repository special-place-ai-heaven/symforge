# A-005 / A-025 — Schema-byte feasibility summary

**Measured:** 2026-06-13  
**Tasks:** T029, T030  
**Raw artifact:** [A-005-schema-bytes.json](./A-005-schema-bytes.json)

## Method

```powershell
.\scripts\measure-schema-bytes.ps1
```

- Binary: `target/debug/symforge.exe` (v7.21.1 build on v8 branch)
- Budget: H1 public ≤ **5,000 B**; edit (A-025) ≤ **1,500 B**
- Measurement: `Buffer.byteLength(JSON.stringify(tools/list result), utf8)`

## Results

| Surface | Status | schemaBytes | Notes |
|---------|--------|-------------|-------|
| `full` (32-tool) | **TODO** | — | MCP probe failed: stderr tracing polluted node capture |
| `compact` (3-tool target) | **TODO** | — | `SYMFORGE_SURFACE=compact` stub **not implemented** |
| `symforge_edit` | **TODO** | — | Not measured; separate tool surface absent |

**Artifact status:** `PARTIAL`

## A-005 verdict

**OPEN — NO-GO for Phase 1 H1 lock**

Reasons:

1. Compact 3-tool surface stub does not exist (`SYMFORGE_SURFACE=compact` filter not shipped).
2. Implementing the stub is **Phase 0.7 non-shipping measurement code** — allowed by gap plan but **not attempted in this session** to avoid product surface changes without sf-bench gate context.
3. Full-surface probe failed due to MCP harness stderr handling (fix tracked in measurement helper, not blocking stub work).

**Pivot if stub cannot land ≤5kB:** slimmer JSON Schema, resource-first reads, or merge edit intents (see stel-assumptions invalidation example).

## A-025 verdict

**OPEN — pivot documented**

**Accepted interim pivot:** merge `symforge_edit` into `symforge` with `intent=edit` until a standalone edit tool schema is measured ≤1,500 B.

Re-measure when:

- Compact surface stub lands, or
- STEL Phase 1 ships `list_tools` filter (post-GO only).

## Next action

1. Fix `measure-schema-bytes.ps1` stderr isolation (RUST_LOG=off or pipe hygiene).
2. Land non-shipping compact stub per gap plan §12A surface-choice note.
3. Re-run script and update this summary with PASS/FAIL vs budgets.
