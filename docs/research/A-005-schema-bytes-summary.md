# A-005 / A-025 — Schema-byte feasibility summary

**Measured:** 2026-06-13 (in-repo gather)  
**Raw artifacts:** [A-005-schema-bytes.json](./A-005-schema-bytes.json), [A-005-schema-bytes-run2.json](./A-005-schema-bytes-run2.json)  
**Probe:** `src/protocol/surface_probe.rs` + `SYMFORGE_SURFACE=compact`

## Method

```powershell
.\scripts\measure-schema-bytes.ps1
.\scripts\measure-schema-bytes.ps1 -OutFile docs/research/A-005-schema-bytes-run2.json
cargo test -p symforge --lib -- surface_probe --test-threads=1
```

Fixture cwd: `tests/fixtures/compression_ratio/rust` (small repo for fast MCP startup)

## Results

| Surface | Tools | schemaBytes | Budget | Pass |
|---------|-------|-------------|--------|------|
| `full` | 32 | 62,574 | informational | — |
| `compact` | 3 | **891** | 5,000 (H1) | **PASS** |
| `symforge_edit` input_schema only | — | **≤1,500** (unit test) | 1,500 (A-025) | **PASS** |

Repeatability (2 runs): compact **891 B / 891 B** → **0.0% variance** (≤2% threshold).

## A-005 verdict

**VALIDATED** — compact 3-tool `tools/list` JSON is **891 B** UTF-8, well under H1 5,000 B budget.

## A-025 verdict

**VALIDATED** — `symforge_edit` input schema passes `symforge_edit_schema_under_a025_budget` unit test (≤1,500 B). Pivot to merged `intent=edit` not required at probe stage.

## Notes

- Probe schemas are draft shapes from `docs/stel-schema.md`; STEL execution not implemented.
- Full 32-tool surface remains **62,574 B** (motivation for compact L0).
