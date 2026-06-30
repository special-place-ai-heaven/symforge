# Contract: Team Index Artifact

**Feature**: 015 · **Sprint**: S1a · **US**: US2  
**Status**: **frozen** 2026-06-30 (S1a Planning Gate — P-S1A-005)  
**Evidence**: EV-S1-003 · **Compression**: zstd (D-015-009)

## Paths

| Artifact | Path |
|----------|------|
| Compressed snapshot | `.symforge/index.bin.zst` |
| Sidecar metadata | `.symforge/artifact.json` |
| Git merge driver hint | `.gitattributes` line for `*.zst merge=ours` |

## Tiers

| Tier | Trigger | Compression |
|------|---------|-------------|
| Fast | Watcher checkpoint / periodic | zstd level 3 |
| Best | `checkpoint_now(export_artifact=true)` | zstd level 9 + optional strip |

## Import flow

1. If local `index.bin` missing AND `index.bin.zst` present → decompress to temp
2. Verify `content_hash` in `artifact.json`
3. Load snapshot via existing `LiveIndex::load`
4. Run stat-check + incremental index for mtime deltas

## Integrity failure

- Quarantine corrupt artifact to `.symforge/quarantine/artifacts/`
- Fall back to full index with health warning

## Constitution

- Artifact is **bootstrap cache**, not query authority after load.
- Byte-exact content preserved (no line-ending normalization).

## Security (R-14 — no secret leak)

The artifact is a snapshot of the LiveIndex; it contains **only what was already
indexed**. The discovery walk (`src/discovery/mod.rs:196–228`) uses
`ignore::WalkBuilder` with default `.hidden(true)` (skips `.env` and any
dotfile/dotdir) **and** respects `.gitignore`. Secrets in git-ignored or hidden
files are never indexed, so they cannot enter `index.bin` or its `.zst` artifact.

- The "best" tier MUST NOT add any path the normal index would exclude.
- Onboarding docs (S6) state the invariant: keep secret files git-ignored; the
  artifact does not re-scan ignored paths.

## Dependencies (implementation)

- Add `zstd` crate to `Cargo.toml` (safe pure-Rust backend; level 3 / 9 per tier).
- Reuse `write_snapshot` / quarantine patterns in `persist.rs` (EV-S1-003).
