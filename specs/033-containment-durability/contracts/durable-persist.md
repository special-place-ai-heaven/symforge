# Contract: Durable persist

**Stories:** US4 (F-004), US7 (F-007), US8 (F-008)
**Implement in:** `src/idempotency.rs` `write_bytes_atomic`; snapshot/artifact keep named tmp but gain `sync_all`

## Sequence

1. Create sibling temp in the destination parent.
2. Write all bytes; flush; `sync_all` the temp file.
3. `persist` / rename onto the destination.
4. Unix: `sync_all` the parent directory. Windows: rename is the replace; parent sync is best-effort/no-op if unsupported.
5. Return `Ok` only after 3 (and 4 on Unix). If 2 or 3 fails: no success, leftover tmp cleaned when possible.

## Surfaces

| File | Writer | Change |
|------|--------|--------|
| `.symforge/index.bin` | `write_snapshot` | `sync_all` before rename; Unix parent sync |
| `index.bin.zst` | `export_artifact` | same |
| `artifact.json` | `export_artifact` | stop in-place `fs::write`; use `write_bytes_atomic` |
| `trust.json` | `write_store`, `write_cli_store` | `write_bytes_atomic` |
| editor MCP JSON | production `cli/init.rs` writers | `write_bytes_atomic` |

Not this contract: `protocol/edit.rs::atomic_write_file` (source lease), test fixtures that `fs::write` temp files.

## Success reporting

`checkpoint_now(verify_after_write=true)` may still re-read via the OS cache. Durability is the sync-before-rename, not the verify re-read. If `sync_all` fails, the checkpoint reports failure.

## Tests (RED first)

- Helper-level: a spy/failpoint or wrapper observes `sync_all` before rename (knowledge-curation already has stage failpoints as precedent).
- Interrupted persist: previous `trust.json` / editor JSON still parseable.
- Existing snapshot round-trip tests still pass.
