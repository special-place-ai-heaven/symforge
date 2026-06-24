# Overlay Redundancy — Decision

Date: 2026-06-24
Worktree: `E:\project\symforge-012` @ `086d6c4` (post-#372 merge)
Method: read-only audit, live-source verification of three independent reports
(Refuter / Vision-historian / Re-derive). No source modified.

## Objective

Decide whether SymForge's per-session base+overlay `IndexView` overlay is
redundant in production, what to do with D15 (PR #372, merged) and D-B0, and
name the real next frontier of the trust campaign.

## Finding

**VERDICT: REDUNDANT (CONFIRMED).** In production today the overlay is never the
sole source of fresh data. Every read sees an edit through the SHARED live index
(single-project `self.index`) or through the refreshed `entry.base`
(cross-project). The overlay is at best a duplicate, never load-bearing. The
refuter could not find a single production read path where the overlay is the
only source of the edit; every escape hatch closes.

This is the production-as-it-stands truth. The vision report's caveat is
accepted and recorded separately: the overlay is also a *deliberate,
spec-mandated seam* (012 US3/SC-003/FR-005) for a future of session-private
edits + in-process embedders (AAP) + per-connection isolation (D16). Both halves
are true. The disposition below preserves the seam while killing the live
redundancy + staleness risk.

## Evidence (verified against live source at 086d6c4)

1. Single overlay writer always advances the SHARED base FIRST, synchronously.
   - `src/protocol/edit_tools.rs:789-796` — `reindex_after_write(&self.index, ...)`
     runs unconditionally for every non-rerouted edit.
   - `src/protocol/edit.rs:390-424` — `reindex_after_write` is fully synchronous;
     its last statement is `index.update_file(...)`. It returns BEFORE any overlay
     touch, so the overlay can never lead the base.
   - `src/protocol/edit_tools.rs:803-818` — the D15 overlay upsert is a strict
     ADDITION after the base write, gated `if let Some(ov) = &self.session_working_set`
     (None on the shared instance / local-stdio). Its own comment (816-817): "the
     base was already updated by reindex_after_write, so reads still see the edit
     via the base fall-through."
   - The other three edit handlers (`edit_within_symbol`, `insert_symbol`,
     `delete_symbol`) call `reindex_after_write` and write NOTHING to the overlay.

2. `self.index` in handlers is the SAME shared Arc as the live project index.
   - `src/daemon.rs:1446` — `index: Arc::clone(&project.index)`. So single-project
     `get_symbol` / `search_symbols` reading `self.index.read()` already see the
     edit.

3. Single-project overlay read is admittedly redundant.
   - `src/protocol/tools.rs:3576-3578` — comment: "a stale overlay falls through
     to the base, which already holds the edit via reindex_after_write (I4)."
   - `tools.rs:3579-3594` — overlay-first probe, base fall-through on miss. Base
     returns identical content. (D15 spec I4: "Redundant, never wrong.")

4. Cross-project refresh always EMPTIES the overlay for any edited project.
   - `src/daemon.rs:766-770` — `refresh_working_set_bases` re-interns
     `entry.base` from the live `project.index` when `Arc::ptr_eq` mismatches.
   - `src/daemon.rs:790-798` — `ws.add(project_id, base)` then attaches a fresh
     EMPTY overlay; `src/live_index/view.rs:896-898` — `add` builds
     `Overlay::fresh(&base)` (empty), "discarding the old overlay's deltas."
   - The warm early-return that PRESERVES overlays (`daemon.rs:775-776`) fires
     only when NO targeted base advanced. But the single overlay writer ALWAYS
     advances the base (`update_file` -> `swap_and_publish` mints a fresh Arc), so
     a populated overlay cannot coexist with a non-advanced base. The
     overlay-preserving path is never reached with a non-empty overlay.
   - The code names this itself — `daemon.rs:792-796` ponytail comment: "correct
     ONLY while overlays are always empty (no production overlay writers)... or
     this silently drops uncommitted deltas on every freshness refresh."

5. No other production overlay consumer exists.
   - `src/daemon.rs:4286-4289` — `session_overlay_has_upsert` is `#[cfg(test)]`.
   - `get_file_content` / `get_file_context` never consult the overlay.
   - `view.rs` IndexView symbol/text/ref branches take the base-only path under
     their `deltas.is_empty()` guards on every production path (D-B0 unreached).

Conclusion: the overlay/IndexView design presupposes SESSION-PRIVATE edits
(visible only in the overlay until commit). The real path writes through to the
shared index immediately. The overlay has no production consumer.

## D15 disposition (PR #372)

**(b) KEEP DORMANT as a vision-aligned seam, BUT remove the single-project
overlay READ (the C4 staleness risk). Keep only the write-seam.**

Justification:
- It is NOT obsolete. It is the literal subject of the 012 spec (US3/SC-003/
  FR-005: per-consumer copy-on-write overlay, session-private edits, in-process
  AAP embedder), tracked-large OPEN with an owner and a stated path. Per the
  stub/seam policy: preserve the seam, stop false success, plan real work. So a
  full REVERT (option a) is rejected — it would delete a deliberate seam.
- BUT option (c) "leave as-is" is wrong: the single-project overlay read
  (`tools.rs:3579-3594`) is not merely redundant, it carries a narrow real
  staleness risk. The overlay holds a snapshot from edit time; if the live index
  advances afterward (external watcher event, a sibling edit), the overlay-first
  read can SHADOW the fresher base. "Redundant, never wrong" holds only while the
  overlay and base are coherent — which the write-through guarantees AT WRITE
  TIME but not for the overlay's whole lifetime. Reading the base directly is
  always at-least-as-fresh.
- Therefore: remove the read short-circuit so `get_symbol` reads `self.index`
  directly (eliminates the shadow risk and the dead branch); KEEP the writer
  (`edit_tools.rs:803-818`, gated `None` on shared/stdio so byte-identical today)
  as the dormant seam plus its `#[cfg(test)]` coherence assertion. This leaves a
  proven write-path seam with zero live read consumer and zero staleness risk.

Honest naming: D15 is NOT "done." It is a **dormant vision seam** — the writer
exists, the read was removed because it was redundant-and-staleness-risky, and
the seam stays inert until a real consumer (precondition below) lands. The
read-path migration plan (Phases 2-5) is PAUSED, not in progress: migrating the
other ~77 `self.index.read()` sites to `IndexView` would be the no-op parity
refactor the migration plan itself warns against (lines 36-48) while edits write
through to the shared base.

Precondition to make the seam load-bearing (records the path to completion):
1. Edits stop writing through to the shared base — `reindex_after_write` gated
   behind an explicit commit/discard; uncommitted edits land in the overlay ONLY
   (data-model `dirty --commit--> rebase`). Until this flips, the overlay can
   only ever duplicate the base.
2. D16 per-connection `/mcp` session isolation — two consumers sharing one base
   to actually exercise SC-003. Today the standalone daemon is single-consumer.
3. D-B0 per-view derived index so non-empty overlays are searchable, and
   `refresh_working_set_bases` switches `ws.add(empty)` to
   `Overlay::rebase(+uncommitted_paths)`.

All three are tracked-large OPEN with owner 012, blocked on the cross-project-
write track, which has no committed sprint.

## D-B0 disposition

**MOOT under the current single-shared-index architecture; keep OPEN as
tracked-large but reframed as BLOCKED on precondition #1 (a real overlay
writer), not on its own implementation.** D-B0 (per-IndexView trigram/reverse
index for non-empty overlays) presupposes overlays that carry deltas no other
path sees. No such overlay exists in production: every IndexView symbol/text/ref
branch takes the base-only path (`deltas.is_empty()`), and
`refresh_working_set_bases` empties overlays on every refresh. Building a derived
index for data that is always empty is infrastructure for its own sake. Do NOT
build it under the single shared live index. It becomes real only when
precondition #1 (write-to-overlay-not-base) lands.

Critically: D-B0 must be DECOUPLED from D13. The ledger currently files D13 as a
"SYMPTOM(B)" beneficiary of "per-IndexView derived indices." That is wrong — D13
is a single-project `xref.rs` extraction defect reachable on the ordinary read
path with zero overlay/view involvement. Attaching it to D-B0 blocks a fixable
field defect behind dormant infrastructure.

## Real frontier (next 1-2 campaign items)

**Item 1 (do first): D13 — xref value/constructor/struct-literal recall.**
Re-classify OUT of Culprit B into its own root ("xref extraction
incompleteness"). It is the only OPEN item with line-pinned, reproduced FIELD
evidence of a silent wrong answer on the most trust-critical operation
("who uses X"): `find_references(MinimalFilter)` returned 2 definition lines and
missed all 5 usages (29% recall). Verified root cause:
- `src/parsing/xref.rs:18` — qualified calls (`MinimalFilter::new()`) capture
  `@ref.call` = the inner identifier (`new`), keyed in `reverse_index` under
  `new`, not the type head. A simple `find_references("MinimalFilter")` never
  scans `qualified_name` head segments.
- `grep struct_expression src/parsing/xref.rs` -> EMPTY (verified). Struct
  literals `MinimalFilter { .. }` are never captured at all.
Scope (contained to `src/parsing/xref.rs` + one lookup branch, NO overlay/view
code):
- Add `(struct_expression name: (type_identifier) @ref.type)` capture.
- In `find_references` for a simple name, ALSO match refs whose `qualified_name`
  head segment == the searched name (one branch — closes constructors for ALL
  types at once). Add per-grammar composite-literal captures (Go `Foo{}`; JS/TS
  `new Foo()` already lands as a call; Python class calls already calls).
- One runnable check: a fixture with `let a: T`, `T::new()`, `T { }`,
  `fn(p: T)` asserting all 4 resolve from `find_references("T")`.

**Item 2: D18/R3 — loud not-found for missing symbols on read.** Same
silent-wrong family as the already-fixed R1/C4 wins: `get_symbol(TotallyFake,
src/main.rs)` returns the full main.rs outline instead of a not-found. Small,
isolated guard at the get_symbol read choke point. High impact, low effort.

**Cheap honesty win (opportunistic, alongside D13): recall-confidence caveat on
type-usage traces.** When `find_references` returns a partial type-usage trace,
the envelope gives no "may be incomplete" signal — a silent degrade that
violates the campaign's loud-refusal principle. Even before D13 is fully closed,
a caveat marks the boundary.

**Defer / do-not-build:** D15 read-migration (Phases 2-5), D-B0, the overlay/
IndexView track — all presuppose a write path the current architecture does not
have. D20 (`search_files` unscoped) is a tiny review-found fix — do
opportunistically (add `path_prefix` field + add to `PATH_PREFIX_FORWARD_TOOLS`).

## Ledger actions (exact records for the SOT)

Record in `docs/reviews/symforge-defect-ledger.md`:

1. **Overlay redundancy (new finding).** "as_of 2026-06-24: the per-session
   base+overlay IndexView overlay is REDUNDANT in production. Every read sees
   edits via the shared live index (single-project `self.index`,
   daemon.rs:1446) or the refreshed `entry.base` (cross-project,
   refresh_working_set_bases empties the overlay, daemon.rs:790-798). Verified
   @086d6c4. The overlay is a deliberate vision seam (012 US3/SC-003), not
   obsolete, but has NO production consumer."

2. **D15** — change status from "PARTIAL (writer landed)" to **"DORMANT SEAM
   (redundant in production)."** Record: writer kept (gated None on shared/stdio,
   byte-identical), single-project overlay READ to be REMOVED (redundant +
   narrow staleness shadow risk). Read-migration Phases 2-5 PAUSED (no-op parity
   refactor until edits stop writing through to the shared base). NOT "done."
   Owner 012. Path to load-bearing = precondition #1 (commit-gated edits) +
   D16 + D-B0.

3. **D-B0** — keep OPEN, reframe from "blocked-on cross-project-write track" to
   **"MOOT under single-shared-index; BLOCKED on precondition #1 (a real
   overlay writer). Do not build until write-to-overlay-not-base lands."**
   DECOUPLE from D13.

4. **D13** — RE-CLASSIFY out of "Culprit B / per-IndexView derived index"
   into its own root **"xref extraction incompleteness."** Mark as the #1 OPEN
   trust defect (HIGH impact x STRONGEST field evidence). Scope: `xref.rs` +
   one `find_references` lookup branch. Not view/overlay-dependent.

5. **D18/R3** — confirm #2 frontier (silent parent-outline on missing symbol,
   silent-wrong family, small guard).

6. **Honesty note** — add: low-recall traces ship no incompleteness signal;
   add a recall-confidence caveat on partial type-usage traces (cheap interim
   honesty win before D13 lands).
