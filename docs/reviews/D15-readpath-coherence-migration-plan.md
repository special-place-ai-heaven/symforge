# D15 — Read-Path Coherence Migration Plan

**Defect:** D15 (Culprit B). Single-project overlay edits are NOT visible in ordinary
reads because the read path reads `self.index` directly (`self.index.read()` →
`&LiveIndex`) instead of going through `IndexView` (which would apply an overlay).

**Status:** OPEN-with-plan. Owner: 012. Blocked-on: a production single-project
**overlay-WRITE path** (see Root, below) — this is the real root and it is OUT of
D15's stated read-migration scope.

**Source of evidence:** investigation workflow `wv4x87n2g` (read-only, 3 agents,
2026-06-24) against `E:\project\symforge-012` @ origin/main `14e1fb8`. Every line
number below was re-grepped in the 012 worktree at that commit; confirm before editing.

---

## The honest finding: D15 cannot be LIVE-verified this sprint

D15's stated acceptance is "prove that a single-project overlay edit becomes visible
in an ordinary read once the read goes through `IndexView`." That cannot be
demonstrated today, for a concrete, evidence-backed reason:

1. **There is NO production single-project overlay-WRITE path.** Every
   `overlay.upsert(...)` / `overlay.tombstone(...)` call in the tree is test-only
   (`src/live_index/view.rs` test module, lines ~1384+). `WorkingSet::add`
   (`view.rs:896-898`) always attaches a **fresh EMPTY** `Overlay`. `daemon.rs:981`
   records the invariant verbatim: *"No overlay is written (invariant)."*
2. **`SymForgeServer` cannot reach an overlay.** The struct that owns all
   protocol-layer reads (`src/protocol/mod.rs:153`) holds only `index: SharedIndex`
   — no `WorkingSet`/`Overlay`/`IndexView` field, and never constructs one. `IndexView`
   is used solely inside `src/daemon.rs` on the cross-project (Feature-012 Phase 3 /
   US1) route, where overlays are **always empty**.
3. **Single-project edits are already visible** via filesystem write + reindex /
   base-swap (`self.index.write()` + ArcSwap publish), NOT via an overlay. So there is
   no overlay delta for a migrated read to surface.
4. **Therefore flipping reads to `IndexView` is byte-identical today.** `IndexView`
   with `overlay: None`/empty short-circuits to the exact base function output
   (`view.rs:228-229, 259-265, 346-366`). A read-surface migration is a
   **parity-preserving refactor with ZERO observable D15 effect** until an overlay
   writer exists.

**Conclusion:** `can_slice_this_sprint = false` for D15's observable criterion. The
missing piece is a single-project overlay-WRITER. Per the global deferral policy this
is a missing **capability** (defer scope with a loud, tracked, owned refusal), not a
defect in something that already claims to work — so D15 stays OPEN-with-plan with this
document as its tracked path-to-completion, and the parity-only read refactor is **not**
landed this sprint as a no-op (it would spend a PR/gate/review cycle changing nothing
observable, and the future overlay-writer may reshape the ideal seam).

---

## Root cause vs symptom

- **Symptom (D15 as stated):** ordinary reads bypass `IndexView`.
- **Root:** no single-project overlay-write seam exists, and `SymForgeServer` has no
  handle to a `WorkingSet`/`Overlay`. Migrating reads without that writer is inert.

The two are coupled: the read migration is only *observable* once a writer can populate
a single-project overlay. So the plan has a **decision gate (Phase 0)** that chooses
whether D15 also builds the writer, before any read flip is worth landing.

---

## Surface (measured)

- **`self.index.read()` sites: 78** — `src/protocol/tools.rs` = 64,
  `src/protocol/edit_tools.rs` = 13, `src/daemon.rs` = 1. The single `daemon.rs:2072`
  site is `Arc::clone(&self.index.read())` inside base construction (the B2 path), NOT a
  tool read — **out of D15 scope**.
- **`capture_*` helpers: 303 occurrences** — methods on `LiveIndex` (defined in
  `src/live_index/query.rs` = 109; `tools.rs` = 56; `format/tests.rs` = 38;
  `persist.rs` = 17; `edit_tools.rs` = 15; `format.rs` = 14). Invoked as
  `guard.capture_xxx(...)` after `self.index.read()`.
- **Mismatch to bridge:** `IndexView` exposes only 5 read methods (`get_file`,
  `all_files`, `search_symbols`, `search_text`, `find_references`). It does NOT mirror
  the ~100 `capture_*` helpers. And `IndexView::get_file` returns `Option<&IndexedFile>`
  (borrowed, tied to the guard) while `capture_shared_file` (`query.rs:1066`) returns
  `Option<Arc<IndexedFile>>` (owned). A naive `guard → view` swap will not compile where
  handlers clone the Arc and drop the guard before formatting — the migration needs a
  small Arc-returning `IndexView` resolver (e.g. `capture_file -> Option<Arc<IndexedFile>>`).

**Parity guarantee that makes the migration safe:** when the overlay is `None`/empty,
every `IndexView` method short-circuits to the base function output. So each flipped site
must be proven byte-identical with `overlay: None` against existing tests/fixtures.

---

## Phased migration plan (each phase ≤5 files, own verification, ordered by leverage)

### Phase 0 — Decision gate (NOT a code slice)
Decide D15's true scope:
- **(a) Re-scope to ALSO add a single-project overlay-write seam** — larger, multi-file,
  touches `SymForgeServer` (give it a `WorkingSet`/overlay) + the edit path (route a
  single-project edit through an overlay before/instead of the base-swap). Only then does
  any read-flip become LIVE-verifiable for D15's stated goal.
- **(b) Land the parity-safe read-surface migration as preparatory work**, and track the
  overlay-writer as the named blocker with an owner.
**Gate:** owner sign-off on (a) vs (b). Until chosen, every slice below is inert.
**Verification:** the grep facts above (no production `overlay.upsert`; `SymForgeServer`
has no overlay field) — both confirmed.

### Phase 1 — Parity-safe single-symbol read flip (lowest blast radius)
- **Files:** `src/protocol/tools.rs` (`get_symbol` single-mode read, currently
  ~3569-3573 — re-grep `guard.capture_shared_file(&params.0.path)`); + possibly a small
  Arc-returning resolver in `src/live_index/view.rs`.
- **Tool:** `get_symbol` (single mode) — one path → one `capture_shared_file` → one
  `IndexedFile` → one formatter (`symbol_detail_from_indexed_file`). The natural seam to
  route through `IndexView::base_only(&guard).get_file(...)` (or the new Arc resolver).
- **Verification:** full gate; **parity assertion** — output byte-identical to pre-flip
  with `overlay: None`; existing `get_symbol` tests stay green. (LIVE D15 visibility is
  BLOCKED until Phase 0 (a).)

### Phase 2 — Single-file content/outline reads
- **Files:** `src/protocol/tools.rs` (`get_file_content` ~6772, `get_file_context` ~3877).
- **Tools:** `get_file_content`, `get_file_context` (same `get_file`-shaped resolution;
  reuse the Phase 1 resolver).
- **Verification:** gate + byte-identical output for normal/missing/binary/metadata-tier files.

### Phase 3 — Reference/dependents reads
- **Files:** `src/protocol/tools.rs` (`find_references`, `find_dependents` handlers).
- **Rationale:** `IndexView` already has overlay-aware `find_references` (`view.rs:571`)
  with documented post-filter semantics — highest-fidelity surface, but the post-filter
  recomputes counts, so higher parity risk.
- **Verification:** gate + assert ordering, `include_filtered`, alias resolution identical
  with `overlay: None`.

### Phase 4 — Search reads (highest parity nuance — do last)
- **Files:** `src/protocol/tools.rs` (`search_text`, `search_symbols`, `search_files`).
- **Risk:** the overlay branches recompute `total_matches` / `overflow_count` /
  `suppressed_by_noise` (`view.rs:513-538`). With `overlay: None` they must equal base
  counts exactly.
- **Verification:** gate + exhaustive count-field parity tests with `overlay: None`.

### Phase 5 — Edit-tool verify reads (deepest; only if Phase 0 chose (a))
- **Files:** `src/protocol/edit_tools.rs` (13 read sites: 531,582,627,865,910,1051,1096,
  1234,1279,1564,1672,1768).
- **Rationale:** these feed the edit/verify pipeline. Only meaningful if single-project
  edits go through an overlay (Phase 0 (a)).
- **Verification:** gate + full edit-tool suite.

---

## Parity risks (guards apply to every phase)
1. Use `IndexView::base_only`/`overlay: None` on EVERY flipped site; never construct a
   non-empty overlay until Phase 0 (a) lands.
2. **Return-type bridge:** add an Arc-returning `IndexView` resolver (or clone the Arc
   inside the view block) — `capture_shared_file` returns an owned `Arc`, `IndexView::get_file`
   a borrow tied to the guard.
3. `search_text`/`find_references` overlay branches recompute counts + apply different
   alias resolution — only with a non-empty overlay; assert counts equal base with `overlay: None`.
4. `IndexView::all_files()` allocates/clones even in the base branch — prefer point
   `get_file` lookups over `all_files` where a handler needs one path.
5. Exclude `daemon.rs:2072` (base construction) and defer `edit_tools.rs` to Phase 5.
6. Preserve the `loading_guard!` early-return — build the view AFTER `loading_guard!(guard)` passes.

---

## Path to completion
- If Phase 0 picks **(a)**: build the overlay-writer (SymForgeServer gets a
  `WorkingSet`/overlay; a single-project edit records an overlay delta), then Phase 1's
  `get_symbol` flip becomes LIVE-verifiable (write a delta → `get_symbol` → observe dirty
  content). That is the genuine D15 fix.
- If Phase 0 picks **(b)**: land Phases 1-4 as byte-identical seam-completion, and keep
  D15 OPEN with the overlay-writer as the named, owned blocker.

**Tracked-large blocker:** single-project overlay-WRITER (the real D15 root). Owner: 012.
