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

## Git-trackability (dogfood finding, 2026-06-30)

Whether the exported artifact is actually **shareable via git** depends
entirely on the *consuming project's own* `.gitignore` — SymForge writes into
`.symforge/` but deliberately does **not** auto-modify a project's `.gitignore`
to un-ignore the artifact files, the same posture CBM's own README takes
("Optional: never committed unless you want it. Add `.codebase-memory/` to
`.gitignore` if you prefer everyone to reindex from scratch."). This is a
team/project decision, not something the tool should force.

**Tradeoff a team should weigh before opting in** (not resolved here, S6 docs
territory): a committed `.zst` is a full new binary blob in git history on
every commit that changes it — non-diffable, permanent, downloaded by every
future clone forever. `.gitattributes: *.zst merge=ours` only prevents merge
conflicts on that blob; it does nothing to bound history growth. Lower-bloat
alternatives exist (a CI artifact cache/release asset instead of git; or
committing only at deliberate checkpoints rather than continuously) and are
worth documenting alongside the plain "commit it" path when S6 writes the
onboarding docs.

Confirmed live against this repo (dogfood, 2026-06-30): `export_artifact`
correctly wrote `.symforge/index.bin.zst` (608 files, 14.85MB → 3.59MB, 4.14×)
+ `artifact.json`, and idempotently created a project-root `.gitattributes`
with the `*.zst merge=ours` hint — exactly as specified. Whether *this*
project (SymForge's own repo) should itself opt into git-tracking its
artifact for contributor onboarding is a separate, low-urgency maintainer
decision, independent of the feature's correctness.
