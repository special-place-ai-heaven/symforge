# D-B0 — Overlay-aware single-project search (`search_symbols` slice)

> **SUPERSEDED (2026-06-24) — MOOT / BLOCKED.** This spec presupposes a
> session overlay that carries deltas no other read path sees. The 012f
> root-cause investigation found that NO such overlay exists in production: the
> single-project read sees edits via the SHARED live index, the cross-project
> path re-interns the base and EMPTIES the overlay, and the only overlay writer
> always advances the shared base first — so the overlay is REDUNDANT, never the
> sole fresh source. D-B0 is MOOT under the single-shared-index architecture and
> BLOCKED on **precondition #1** (a real overlay writer — edits commit-gated to
> the overlay, NOT written through to the shared base). Do NOT build under the
> current architecture. Authoritative decision:
> `docs/reviews/overlay-redundancy-decision.md`. Body kept below as a future
> reference for when precondition #1 lands.

Status: SLICEABLE. Mechanism: PER-DELTA SCAN + MERGE (no derived index).
Lands on top of D15 overlay-WRITER (commit 086d6c4, PR #372).

---

## objective

Make single-project `search_symbols` read-your-writes: after a D15 edit in a
session, a `search_symbols` call IN THE SAME SESSION must reflect the edit
(renamed/added symbols appear; deleted/renamed-away symbols disappear) instead
of the stale base.

Concretely: route the single-active-project `search_symbols` handler through
the session's per-project `IndexView` (overlay-first), mirroring exactly what
D15 did for `get_symbol` (tools.rs:3579-3586). When the overlay is empty (the
shared instance, local-stdio, or a session with no edits) the result is
byte-identical to today's base search via `IndexView`'s empty-overlay fast path
(view.rs:349-366 → `search_symbols_with_options`).

The merge logic ALREADY EXISTS and is tested: `IndexView::search_symbols`
(view.rs:340-410). The slice is (1) WIRING the single-project handler to it,
plus (2) closing the `// DEFERRED (D-B0)` option-surface gap (view.rs:332-337)
so the overlay-present branch honors the same `SymbolSearchOptions` (path scope,
language, noise, result_limit ranking) the base branch applies — required for
single-project parity, since single-project callers pass real options that the
cross-project path never exercised.

## non_goals

- `search_text` and `find_references` overlay-awareness. The `IndexView` merge
  for both exists (view.rs:452, 571) and is tested, but each is a SEPARATE
  wiring slice with its own AC (text exactness gaps view.rs:441-451; reference
  alias-resolution gap view.rs:565-568). Out of scope here; named follow-ups.
- Cross-project search overlay-awareness. Already wired (Report A §5,
  `execute_cross_project_read` → `WorkingSet::search_symbols`); only ever runs
  with empty overlays today. Not touched.
- Building any per-overlay derived index (trigram / reverse-ref / symbol map).
  Explicitly rejected — see "mechanism" below.
- Fixing `refresh_working_set_bases` empty-overlay-drop. NOT needed for this
  single-project slice (see prerequisite). Named follow-up for the cross-project
  extension.
- Performance work. The dirty set is 1-few files; a linear scan is bounded.

## allowed_files

- `src/protocol/tools.rs` — `SymForgeServer::search_symbols` (4526): add the
  overlay-first branch that builds an `IndexView` from `self.session_working_set`
  for the active project and calls `IndexView::search_symbols`, falling through
  to the existing `self.index.read()` base path when no overlay/no deltas.
- `src/live_index/view.rs` — close the `// DEFERRED (D-B0)` gap in
  `IndexView::search_symbols` overlay-present branch (332-409): apply the full
  `SymbolSearchOptions` surface (path scope, language filter, noise policy,
  result_limit) to the merged hit set, matching `search_symbols_with_options`
  (search.rs:824-913). Tests in the same file.
- `src/daemon.rs` — TEST ONLY: add the D-B0 production-path AC test alongside
  the D15 tests (4300+). No production daemon change.

Forbidden: `search.rs` engine functions (reused as-is via the fast path),
`edit_tools.rs`, `mod.rs`, cross-project routing (daemon.rs:3175-3473),
`refresh_working_set_bases`.

## mechanism — PER-DELTA SCAN, not a derived index

DECISION: per-delta scan + merge. No prebuilt overlay index.

Justification (Report B, per-file cost evidence):
- Symbol search has NO prebuilt map even on the base — `search_symbols_with_options`
  (search.rs:808) is a per-file linear scan of `IndexedFile.symbols`
  (search.rs:852). There is no symbol index in `LiveIndex` or `DerivedIndices`.
  So "D-B0 derived index" is a MISNOMER for `search_symbols`: there is nothing
  to derive. The overlay branch already scans `all_files()` (view.rs:374) —
  the same per-file scan the base does, over the overlay-resolved set.
- A base derived structure (trigram, reverse-ref) exists only to amortize
  repeated queries over a large mostly-static corpus (thousands of files).
  The overlay is 1-few files and changes every edit; building a derived index
  over it costs as much as scanning it once. Per-delta scan is strictly
  simpler and the same cost.
- `IndexedFile` already carries `symbols: Vec<SymbolRecord>` per delta
  (store.rs:236) — no extra payload needed.

ponytail: the laziest correct mechanism is the one already in `view.rs`. The
slice REUSES `IndexView::search_symbols` rather than re-implementing a merge in
the handler.

## contracts_or_interfaces

- `IndexView::search_symbols(query, kind_filter, &SymbolSearchOptions) -> Vec<ViewSymbolHit>`
  (view.rs:340) — already the contract. After closing the deferred gap, the
  overlay-present branch returns the SAME shape and honors the SAME options as
  the empty-overlay fast path.
- `SymForgeServer::search_symbols` (tools.rs:4526) — public behavior unchanged
  for the no-overlay case (byte-identical). For the overlay case it returns the
  merged result. Output formatting, `result_status`, browse-mode ranking
  (tools.rs:4569+), and `hidden_search_symbols_noise_count` all operate on the
  merged hit set unchanged.
- Overlay lookup mirrors `get_symbol` (tools.rs:3579): read
  `self.session_working_set`, get the entry for `ov.project_id`, build
  `IndexView::new(&entry.base, Some(&entry.overlay))`. On `Err(StaleOverlay)`
  (fence mismatch) fall through to base-only — correct, the base already holds
  the edit (Report C §4, spec I4).

## invariants

- SHADOW RULE: for every path P present in `overlay.deltas`, base hits for P are
  excluded; the overlay version's hits are included. Enforced by `all_files()`
  (view.rs:288-308): overlay upserts shadow base, then the scan runs over the
  resolved set. (Symbol search resolves the whole set rather than retain-drop,
  but the net is identical: a shadowed path's symbols come only from the
  overlay copy.)
- TOMBSTONE: a `FileDelta::Tombstone` path contributes zero hits — `all_files()`
  drops it from the resolved set entirely (view.rs:288-308). Base hits for that
  path are gone; no overlay hits added.
- NO DOUBLE-COUNT: each path appears at most once in `all_files()` (overlay key
  wins over base), so no symbol is counted from both base and overlay.
- LOCK HIERARCHY (mirror D15, tools.rs:3573-3578): take the overlay `read()`,
  build/consume the `IndexView`, and DROP the overlay guard BEFORE taking
  `self.index.read()` on the fall-through. No index-lock/ws-lock nesting (I2);
  no daemon-map lock held (I1). NOTE: `IndexView` borrows `&entry.base` and
  `&entry.overlay` — the overlay `read()` guard must outlive the `IndexView`,
  so the merge result must be fully materialized (owned `Vec<ViewSymbolHit>`,
  which `search_symbols` already returns) before the guard drops.
- SINGLE-PROJECT PARITY: with an EMPTY overlay (or `None`
  `session_working_set`), the result is byte-identical to today's base search.
  Guaranteed by the empty-overlay fast path (view.rs:349-366) which delegates to
  the identical `search_symbols_with_options`. After closing the deferred gap,
  the NON-empty branch honors the same option surface (path scope, language,
  noise, result_limit) so a non-empty overlay does not silently change scoping
  behavior versus the base for unedited files.

## acceptance_criteria

AC1 (PRODUCTION-PATH read-your-writes — false-success guard built in): mirror
`test_d15_overlay_read_your_writes_via_execute_tool_call` (daemon.rs:4300).
Open a real session via `open_project_session`; fixture `src/lib.rs` with
`pub fn target() -> u32 { 1 }`. Via `execute_tool_call(runtime, "replace_symbol_body", ...)`
RENAME the symbol so the edited overlay version contains a name that exists
ONLY in the overlay (e.g. edit the symbol body/signature to define
`target_overlay_only`), making the overlay the sole source of that name. Assert
`session_overlay_has_upsert` is true (proves the overlay branch is live). Then
`execute_tool_call(runtime2, "search_symbols", {"query":"target_overlay_only"})`
in the SAME session and assert:
  - the result CONTAINS `target_overlay_only` (came from the overlay merge — the
    base index has no such symbol, so a base-only read would return zero hits);
  - this is the overlay-specific guard: a symbol present ONLY in the edited
    version, not incidentally in the base.

AC2 (tombstone / rename-away disappears): in the same edited state, a
`search_symbols` for the ORIGINAL name that the edit removed must NOT return the
stale base hit for that path. (If a clean rename is awkward through
`replace_symbol_body`, assert via an added-symbol variant: the base symbol that
the overlay file no longer defines is absent.) Drives the shadow rule.

AC3 (no-overlay parity — byte-identical): a `search_symbols` call on a session
with an EMPTY overlay (no edit) returns exactly what the base path returns.
Assert equality of the formatted output against the same query run with
`session_working_set = None` (or assert the empty-overlay fast path is taken).

AC4 (option-honoring on the overlay branch): with a non-empty overlay, a
`search_symbols` call with a `path_prefix` / `kind` / language filter must apply
that filter to the merged set (close the `// DEFERRED (D-B0)` gap). Unit test on
`IndexView::search_symbols` directly (view.rs tests) with a path-scoped option
and a dirty file inside vs outside the scope.

AC5 (no cross-session leak): mirror `test_d15_overlay_no_cross_session_leak`
(daemon.rs:4387) — edit in session A, `search_symbols` in session B for the
overlay-only name returns nothing (B's overlay has no delta; B reads base).

## evidence_required

- Output of `cargo test --all-targets -- --test-threads=1` showing AC1-AC5
  passing (the new tests + the existing D15 + view.rs overlay tests still
  green: `view_search_symbols*`, `view_search_text_reflects_overlay_deltas`,
  `proof2_overlay_isolation`).
- The AC1 test asserting the overlay-only symbol name in the search output AND
  `session_overlay_has_upsert == true` (production path through
  `execute_tool_call`, real `SessionRuntime`).
- `cargo clippy --all-targets -- -D warnings` clean on the touched files.
- `cargo fmt --check` clean.

## stop_conditions

- If `IndexView::search_symbols` cannot honor the full `SymbolSearchOptions`
  surface without duplicating large swaths of `search_symbols_with_options`
  (search.rs:824-913) — i.e. the merge would have to fork the engine's option
  logic — STOP and reconsider: prefer extracting the per-file option-gate from
  the engine into a shared helper both branches call, rather than copy-paste.
  Flag as a design decision, do not silently duplicate.
- If wiring reveals the active project is NOT seeded into the session working
  set (the `entry == None` case noted in edit_tools.rs:814) — that is the D15
  seeding bug, not a D-B0 concern; STOP and report rather than papering over.
- If `IndexView` borrow-lifetime forces holding the overlay guard across
  `self.index.read()` (lock-nesting violation) — STOP; the materialize-then-drop
  ordering must hold (see lock-hierarchy invariant).

## prerequisite (NAMED, NOT required for this slice)

`DaemonState::refresh_working_set_bases` (daemon.rs:735, step 4 ~790) calls
`ws.add(project_id, base)`, which attaches a FRESH EMPTY overlay and DISCARDS
uncommitted deltas (view.rs:894-908). Confirmed (Report C §3) this fires ONLY
on the cross-project route; the single active project bypasses it
(`!targets_is_single_active` gate, daemon.rs:3229-3233; `resolve_cross_project_targets`
returns `None` for single-active, daemon.rs:2989). Therefore THIS single-project
slice does NOT need it.

It becomes a HARD prerequisite for the LATER cross-project D-B0 extension (or any
cross-project overlay writer): switch `ws.add` to
`Overlay::rebase(&new_base, &still_dirty)` (view.rs:190, already exists + tested)
where `still_dirty = Repository::uncommitted_paths()` (git.rs:68). Owner: the
cross-project-search follow-up. Tracked, not deferred-as-defect (this slice does
not rely on it).

## verification_command

```
cd /e/project/symforge-012 && CARGO_INCREMENTAL=0 cargo test --all-targets -- --test-threads=1 \
  && CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings \
  && cargo fmt --check
```

Targeted during development:
```
cd /e/project/symforge-012 && CARGO_INCREMENTAL=0 cargo test d_b0 -- --test-threads=1 --nocapture
cd /e/project/symforge-012 && CARGO_INCREMENTAL=0 cargo test view_search_symbols -- --test-threads=1
```

## follow-ups (named, owned)

- D-B0-text: wire single-project `search_text` (tools.rs:4734) through
  `IndexView::search_text`; close the `total_matches`/`overflow_count` exactness
  gap (view.rs:441-451).
- D-B0-refs: wire single-project `find_references` through
  `IndexView::find_references`; close the alias-map resolution gap across dirty
  files (view.rs:565-568).
- D-B0-xproj-prereq: `refresh_working_set_bases` → `Overlay::rebase` before any
  cross-project non-empty overlay is relied on.
