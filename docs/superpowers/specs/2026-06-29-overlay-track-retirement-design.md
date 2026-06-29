# Overlay track retirement (precondition #1 — DO NOT BUILD)

**Date:** 2026-06-29
**Lane:** `E:\project\symforge-012` (SymForge trust campaign)
**Status:** decision approved (user); Phase 1 implementation pending.

## Decision

**Do NOT build precondition #1** (the commit-gated session-private overlay
writer). **Retire the overlay-writer track** instead. Session-private in-index
edits are **superseded by a superior mechanism** and have **no consumer**.

### Why (three converging investigations, all code-verified)

1. **No consumer.** The overlay's only distinguishing value over the existing
   shared index is cross-session isolation = D16 (multi-tenant `/mcp`), which the
   user deselected and which has no committed sprint. For single-session
   speculative edits, a per-file undo-log over the shared index is cheaper AND
   more capable (every read verb already reads `self.index`, so an in-memory
   `update_file` is visible everywhere for free; the overlay gives only
   `get_symbol` visibility and punts search to a loud refusal).
2. **AAP — the named in-process consumer — does NOT need it (Verdict B).** AAP
   embeds SymForge via `features=["embed"]` but isolates edits at the
   **git-worktree** (host: one worktree per task, `task/{project}/{task}`) and
   **Firecracker-VM** (guest: one in-process index per VM) boundaries. Its edit
   model is uniformly **commit-then-reindex** (`update_file` after a real disk
   write); it never feeds uncommitted edits to a shared index, and there is zero
   overlay/speculative design intent in `crates/`, `specs/`, or `docs/`. Evidence:
   `aap-code-intel/src/adapter.rs:63,1187-1219`, `aap-agents/src/actors/coder_actor.rs:283-361`,
   `aap-git/src/worktree_manager.rs:62-121`, `aap-guest-agent/src/code_intel.rs`.
3. **The goal is already met by a better approach.** The goal — isolated
   concurrent edits — is delivered by OS-level worktree/VM/process isolation,
   which is *strictly stronger* than an in-memory overlay (can't leak via a logic
   bug, survives crashes, no commit/discard bookkeeping). SymForge's architecture
   has consistently chosen coarse-grained isolation (process, worktree, VM) over
   fine-grained in-index isolation. Session-private in-index edits is superseded,
   exactly as multi-hop decomposition was superseded by the client agentic loop.

The no-fake policy forbids leaving the D15 writer seam dormant: it must either get
a real consumer (build precondition #1) or be removed. There is no consumer, so it
is removed.

## Phase 1 — remove the dead session-private-edit writer track

Scope confirmed by call-site analysis @ origin/main. The ONLY production caller of
`Overlay::upsert` is the writer seam; `tombstone`/`rebase` have zero production
callers; `parse_indexed_for_overlay` is called only by the seam;
`session_working_set` is set only in `session_runtime()` and read only by the seam.

**Remove:**
- `src/protocol/edit_tools.rs:810-825` — the overlay-writer seam.
- `src/protocol/edit.rs:438-462` — `parse_indexed_for_overlay` (only the seam calls it).
- `src/protocol/mod.rs:152-162,206,288,319` — `SessionOverlay` struct + the
  `session_working_set` field + its `None` initializers.
- `src/daemon.rs:1501-1513` — the `session_runtime()` code that sets
  `session_working_set = Some(...)`.
- `src/live_index/view.rs` — `Overlay::{upsert,tombstone,rebase}` (test-only
  after the seam goes) + their unit tests.
- The `#[cfg(test)]` overlay-writer coherence assertions/helpers
  (`daemon.rs` `session_overlay_has_upsert`, `test_session_working_set_*`,
  `view.rs` overlay-mutator tests).
- `docs/reviews/D15-readpath-coherence-migration-plan.md` — the paused plan
  (superseded; delete or mark superseded).

**Keep (LIVE — the cross-project find-fusion read path):** `IndexView`, the
`Overlay` struct + `Overlay::fresh`, `WorkingSet`/`WorkingSetEntry`,
`refresh_working_set_bases` (B2 cross-project freshness), the cross-project read
path. After Phase 1 these still construct `IndexView` with an always-empty overlay
— byte-identical behavior.

**Record:** ledger D15 → SUPERSEDED/REMOVED (writer track retired; session-private
edits superseded by worktree/VM/process isolation); D-B0 → WON'T-BUILD under the
single-shared-index architecture (was MOOT/parked; now formally retired with a
revival trigger); D16 → decoupled, kept as an INDEPENDENT future feature (its own
coarse-grained per-connection isolation if multi-tenant serving is ever wanted),
NOT blocked on the overlay. Note the supersession against 012 spec US3/SC-003/FR-005.

**Revival trigger (capability deferral, not silent drop):** build precondition #1
only if a future architecture requires one shared index to reflect multiple
consumers' uncommitted pre-merge edits simultaneously (accepting weaker isolation
than worktree/VM). Build it then, in the protocol-free embed module, with that
consumer in hand.

## Phase 2 (separate, reviewed follow-on — NOT this PR)

Collapse the now-provably-empty overlay scaffolding out of the LIVE cross-project
path: drop the overlay field from `WorkingSetEntry`, simplify `IndexView` to
base-only, remove the unreachable `!deltas.is_empty()` overlay-read branches and
the `Overlay` struct. Bigger refactor of a live path; its own design + adversarial
review. Phase 1 stands alone and is byte-identical for all reads.

## Acceptance (Phase 1)

- AC1: `rg 'session_working_set|SessionOverlay|parse_indexed_for_overlay'` over
  `src/` → empty; `Overlay::{upsert,tombstone,rebase}` gone.
- AC2: cross-project find-fusion read path unchanged in behavior (IndexView still
  built with an empty overlay); `tests/stel_find_fusion.rs`, `tests/cochange_fusion.rs`,
  and all cross-project/daemon freshness tests (B2) green.
- AC3: full gate green — `cargo fmt --check`, `cargo check`,
  `cargo clippy --all-targets -- -D warnings`,
  `cargo test --all-targets -- --test-threads=1`, `cargo build --release`.
- AC4: ledger + spec supersession recorded; revival trigger stated.
