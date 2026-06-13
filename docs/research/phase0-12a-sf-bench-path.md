# Phase 0 §12A — evidence source (in-repo)

**Updated:** 2026-06-13  
**Primary source:** SymForge repository (no external sf-bench required)

## Operator note

External **sf-bench** was a legacy sibling-repo battery (cloned test repos, token comparison across tools). It is **not required** for ongoing evidence collection. Equivalent Phase 0 evidence is gathered via:

| Capability | In-repo path |
|------------|--------------|
| Schema bytes (H1) | `scripts/measure-schema-bytes.ps1` + `src/protocol/surface_probe.rs` |
| Competent manual M | `src/protocol/format.rs` + unit tests |
| MCP shakedown | `docs/research/A-003-mcp-shakedown.jsonl` |
| Gate preflight summary | `docs/research/G-005-inrepo-preflight.json` |
| Gather script | `scripts/gather-phase0-evidence.ps1` |
| Test fixtures | `tests/fixtures/compression_ratio/` |

## Legacy external path (optional)

If sf-bench is restored later (`E:\project\sf-bench` or sibling clone), it can supplement full 36-row battery replay. **Not blocking** Phase 0 schema/manual/shakedown evidence.

## Blocker status

**B-SFBENCH:** **CLOSED** — superseded by in-repo evidence path (2026-06-13).

Full multi-repo battery (A-001 session_net, A-004 equiv audit, A-028 golden corpus) remains **OPEN** until in-repo golden file + replay land or operator deprioritizes in gap plan.
