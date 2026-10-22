# Follow-up: structural edit cannot repair its malformed postimage

Status: open; recorded 2026-10-20 during AAP S14-S16 dogfooding. Separate from that implementation.

Observed MCP runtime: SymForge v11.9.1. File: `src/index_lifecycle/embed_query.rs`, function `execute_inner`.

## Reproduction observed

1. `edit_within_symbol` inserted the opening `if matches!(snapshot_mode, SnapshotMode::Strict) {` before the commitment loop. The closing brace was intended as a later edit.
2. The first edit succeeded and left a temporary syntax error.
3. The next `edit_within_symbol` could not resolve `execute_inner` and refused the closing-brace repair: `Symbol not found: fn execute_inner (file partially parsed with errors ...)`. The tree-sitter diagnostic pointed at the module documentation on line 1.
4. `get_file_content` still worked. An exact one-brace fallback patch restored parsing; `analyze_file_impact` refreshed the index.

## Fix investigation and acceptance

Investigate whether structural edits should reject invalid postimages before writing, or provide a guarded recovery edit when the original target no longer parses. Check existing atomic `batch_edit` support before adding another API. Improve diagnostic location if feasible.

A regression must prove that a rejected malformed edit preserves the original file bytes, or that a guarded recovery can repair this case without bypassing ownership, concurrency, and edit-safety checks.

Workaround: replace the complete well-formed symbol, or apply a complete atomic edit batch; avoid partial scope-opening edits.
