# Spec 026 — serve snapshot restore

**Status**: implemented in the same PR (one-campaign-one-PR, per
`specs/024-optimization-backlog/BACKLOG.md` protocol).
**Origin**: 024 backlog items 8/9 research pass (2026-08-04), recorded in
BACKLOG.md §"Research result — serve snapshot restore".

## Problem

`symforge serve` never consulted the persisted `.symforge/index.bin`
snapshot: every serve start paid the full cold pipeline (release, this repo:
parse 4.60 s + runtime 3.59 s + publication 2.67 s + trigram 0.61 s ≈ 9.9 s
to listening), while the stdio local path already restores the same snapshot
in well under a second and reconciles in the background. The gap made serve
starts fragile against the 60 s daemon/admin start deadline and re-parsed
work that was already durably persisted.

## Requirements

- **FR-026-1**: `load_serve_index` MUST try `persist::load_snapshot` (the
  staleness-gated loader every other consumer uses) before the synchronous
  cold `LiveIndex::load_for_state_placement`. A usable snapshot is rehydrated
  via `snapshot_to_live_index_with_code_signals` and published through the
  same `SharedIndexHandle` path the stdio restore uses.
- **FR-026-2**: A snapshot-restored serve index MUST spawn
  `persist::background_verify` with the snapshot's mtime map — identical to
  the stdio flow — so disk drift is reconciled without blocking startup, and
  the index carries the honest `SnapshotRestore` / `Pending` trust labels
  until verification completes.
- **FR-026-3**: No snapshot (absent, stale, wrong root) keeps the exact prior
  behavior: synchronous cold load, same error surface (`ServeError::IndexLoad`),
  no verify task. Unbound roots keep serving the empty index.
- **FR-026-4**: No new staleness policy is introduced — the gate lives solely
  inside `load_snapshot`, shared with stdio/daemon (single decision point).

## Success criteria

- **SC-026-1**: `load_serve_index_restores_snapshot_when_present`
  (`src/server/serve.rs` tests) — cold path with no snapshot returns no
  verify map; after `checkpoint_shared_index`, the warm path returns a
  `SnapshotRestore`-labeled index with the mtime map.
- **SC-026-2**: full serial suite green (regression gate — serve HTTP attach,
  auth, port batteries unchanged).
- **SC-026-3**: measured claim: warm serve start reaches "index ready" without
  a parse phase (log line `serve: loaded serialized index from
  .symforge/index.bin`); cold-parse timing applies only to genuinely cold
  first indexes.
