# D15 Overlay-WRITER — Mini-Spec (CORRECTED v2)

> **SUPERSEDED (2026-06-24, post-merge).** The single-project overlay READ this
> spec describes (get_symbol serving overlay content as "read-your-writes",
> sections C4 / objective / C1) was REMOVED in `fix/012f-overlay-redundancy`: it
> is redundant (the shared live index already holds the edit via
> `reindex_after_write`) and carried a narrow staleness shadow risk. The overlay
> WRITER is kept as a dormant 012-spec seam with no production read consumer.
> See `docs/reviews/overlay-redundancy-decision.md` for the verified finding and
> disposition. This document is retained as the historical write-seam design;
> treat any read-path claim below as HISTORICAL, not current behavior.

**Date:** 2026-06-24
**Status:** SUPERSEDED — write-seam landed (dormant); the single-project overlay
READ was removed (redundant + staleness). Originally a REDESIGN of the prior
NEEDS_REDESIGN draft (fixed the CRACK — Attack 4, project_id vs project_name key —
and the FATAL — Attack 6, shared-server overlay leak / false-success).
**Owner:** 012

---

## Why the redesign (the two flaws and their fix)

### CRACK (Attack 4) — WorkingSet is keyed by the hash, not the display name

Ground truth (verified in worktree E:\project\symforge-012):

- `WorkingSet` entries are looked up by `project_id` — `WorkingSet::get`/`get_mut`
  match `e.project_id == project_id` (view.rs:921/926/904).
- The key stored is the hash: every `working_set.add(...)` call passes a
  `project_key(root)` result (daemon.rs:855, 1239), and
  `project_key` returns `format!("project-{}", digest_hex(...))` (daemon.rs:3640-3651).
- `SymForgeServer` carries ONLY `project_name` (the display name, e.g. "symforge");
  it is constructed with `project_name.clone()` and never the hash (daemon.rs:2157-2163,
  mod.rs:157).

So the prior spec's `ws.get(&self.project_name)` would ALWAYS MISS in production —
a dead overlay branch. **Fix:** the overlay handle must carry the hash `project_id`,
which `session_runtime()` already has as `session.active_project_id`
(daemon.rs:1444). The lookup keys on that hash, never on `project_name`.

### FATAL (Attack 6) — a shared-server field leaks across sessions

`ProjectInstance.server` is a SINGLE `SymForgeServer` shared across every session of
that project. `session_runtime()` builds the per-call `SessionRuntime` with
`server: project.server.clone()` (daemon.rs:1449). `SymForgeServer` is
`#[derive(Clone)]` with EVERY field `Arc`-wrapped (mod.rs:152-211). Therefore a
field of type `Arc<RwLock<...>>` would be SHARED by the clone — setting it on the
clone still mutates `project.server`, leaking session A's overlay into session B
(SC-003 cross-session leak), and the daemon HTTP path (`execute_tool_call` using
`runtime.server`) would never set it at all → unit test green while production
overlay code is permanently skipped (false success).

**Fix (Option 3, plain Option shape):** the field is `Option<...>` BY VALUE (NOT
`Arc`-wrapped at the field level — the `Arc` is INSIDE the `Option`). `#[derive(Clone)]`
gives each clone its OWN `Option` slot. Setting `Some(...)` on the
`SessionRuntime`-owned local clone in `session_runtime()` does NOT touch
`project.server` (which keeps `None`). No cross-session leak by construction. This
is leak-safe ONLY with the plain-`Option` shape; an `Arc`-wrapped field reintroduces
SC-003.

### Bridge-fix decision: Option 3, not Option 1

Option 1 (thread `working_set` as a parameter) is rejected on evidence:
`replace_symbol_body` and `get_symbol` are `&self` methods and
`replace_symbol_body` is `#[tool]`-macro-locked (rmcp generates the router from the
fixed `(&self, Parameters<Input>)` signature, edit_tools.rs:573 / tools.rs:3439).
Adding a param forces a private-inner + thin-shim split, ripples to ~35 call sites
(21 `.replace_symbol_body(`, 6 `.replace_symbol_body_tool(`, 8 `.get_symbol(`,
mostly tests), AND the stdio `#[tool]` router path still cannot pass the overlay —
a built-in second false-success surface.

Option 3 is the smaller leak-safe diff: 2 source files (`mod.rs`, `daemon.rs`),
2 method bodies edited (`replace_symbol_body`, `get_symbol` single-mode — internal,
NO signature change), 0 call sites updated, no macro fight.

Note on the "do it in execute_tool_call directly" idea (Report A point 5): the
overlay handle IS already reachable there as `runtime.working_set`, but the
upsert SOURCE (the freshly parsed `Arc<IndexedFile>`) is produced INSIDE
`replace_symbol_body` (via `reindex_after_write`'s parse), and the read consult must
happen INSIDE `get_symbol`'s single-mode capture. Doing it in `execute_tool_call`
would require re-parsing the file there (duplicate work) and re-implementing the
read fallback. Carrying the handle to the methods via the per-session-clone field is
strictly less code and keeps the logic where the parse already lives. Option 3 wins.

---

## objective

After `replace_symbol_body` writes to disk (single-project, active project), ALSO
upsert the freshly parsed `Arc<IndexedFile>` into the SESSION's per-project overlay,
keyed by the session's hash `project_id`. A subsequent `get_symbol` single-mode call
in the SAME session then serves the overlay content (read-your-writes), proving the
overlay path is live. This is the prerequisite for the D15 read-path migration
(flipping read tools to consult `IndexView`).

---

## non_goals

- Other edit tools (`insert_symbol`, `delete_symbol`, `edit_within_symbol`,
  `batch_*`): NOT in this slice. Extend after this proves out.
- Read-path migration beyond `get_symbol` single-mode: separate work.
- Cross-project overlay writes: deferred. This slice touches ONLY the active
  project's overlay entry.
- Overlay persistence across sessions / across daemon restarts: out of scope.
- Overlay rebase on base-swap: out of scope. A stale overlay falls through to the
  base, which `reindex_after_write` has already updated (see I4).
- Removing `reindex_after_write`: NOT done. The base-swap must still happen so the
  watcher and all non-overlay reads stay correct.

### KNOWN CEILING — D-B0 prerequisite (Attack 5, NAMED, not silent)

`DaemonState::refresh_working_set_bases` (daemon.rs:735) step (4) at daemon.rs:790-797
calls `ws.add(project_id, base)`, which attaches a **fresh EMPTY overlay** re-fenced
to the new base — silently discarding any uncommitted overlay deltas. There is
already a `ponytail:` comment at daemon.rs:792-796 naming this exact ceiling.

For THIS single-project slice it is HARMLESS: `refresh_working_set_bases` is invoked
only on the cross-project read freshness gate (`call_tool_handler`, daemon.rs:2581,
guarded by `resolve_cross_project_targets`), which fires ONLY for multi-target reads.
A single-active-project session never triggers it, so the overlay written here is
never dropped by a refresh.

It IS a NAMED GATING PREREQUISITE for D-B0 (cross-project overlay writes): before any
cross-project overlay writer lands, `refresh_working_set_bases` step (4) must switch
from `ws.add(empty overlay)` to `Overlay::rebase(+ uncommitted paths)` or it will
silently drop uncommitted deltas on every freshness refresh. Owner: D-B0. Tracked,
not deferred-as-acceptable.

---

## allowed_files

Exactly 2 source files (Option 3 bridge-fix), plus this spec and tests:

1. `src/protocol/mod.rs` — add ONE field to `SymForgeServer`
   (`session_working_set: Option<SessionOverlay>`, plain `Option`), and `None`
   initializer in `SymForgeServer::new` and `SymForgeServer::new_daemon_proxy`.
   Add the `WorkingSet`/`RwLock` import.
2. `src/daemon.rs` — in `session_runtime()` (the SINGLE production
   `project.server.clone()` site, daemon.rs:1449) set the field on the LOCAL clone
   from `session.active_project_id` (hash) + `Arc::clone(&session.working_set)`.
3. `src/protocol/edit_tools.rs` — overlay upsert inside `replace_symbol_body`
   (internal; NO signature change), reusing the parse from the write path.
4. `src/protocol/tools.rs` — `get_symbol` single-mode read consults the overlay
   first (internal; NO signature change).
5. Tests: new integration test driving `execute_tool_call` with a genuine
   `SessionRuntime` (see acceptance_criteria AC1 / evidence_required).

`edit_tools.rs` and `tools.rs` edit ONLY the two named method bodies — no
signature changes, no call-site churn, no `#[tool]` macro change.

---

## contracts_or_interfaces

### C1 — SymForgeServer carries an optional per-session overlay handle (KEYED BY HASH)

```rust
// src/protocol/mod.rs — small helper type carrying BOTH the hash key and the ws handle.
// Plain struct in an Option; the Arc is INSIDE, so #[derive(Clone)] on SymForgeServer
// gives each clone its own Option slot (leak-safe, see I7/SC-003).
pub(crate) struct SessionOverlay {
    /// The HASH project_id (project_key result), NOT the display project_name.
    /// This is the WorkingSet lookup key — fixes the CRACK.
    pub(crate) project_id: String,
    pub(crate) working_set: Arc<parking_lot::RwLock<crate::live_index::WorkingSet>>,
}

// inside SymForgeServer struct:
/// Per-session read-your-writes overlay handle. `None` on the shared
/// `project.server` instance and in local-stdio mode; `Some` ONLY on the
/// per-session clone built in `session_runtime()`. Plain `Option` (Arc is INSIDE)
/// so the derive(Clone) gives each clone its own slot — setting it on the local
/// clone never mutates the shared instance (SC-003 leak-safe by construction).
pub(crate) session_working_set: Option<SessionOverlay>,
```

`None` everywhere except the session-runtime clone. Local-stdio reads/writes are
byte-identical to today (the field is `None`; both paths fall through).

### C2 — Wire the field on the LOCAL clone in session_runtime() (FATAL fix)

`src/daemon.rs` `session_runtime()`, replacing `server: project.server.clone(),`
(daemon.rs:1449):

```rust
server: {
    let mut s = project.server.clone();   // owned clone; project.server untouched
    s.session_working_set = Some(SessionOverlay {
        project_id: session.active_project_id.clone(), // HASH key (CRACK fix)
        working_set: Arc::clone(&session.working_set),
    });
    s
},
```

This is the SINGLE production `project.server.clone()` site (verified: the only
other `SymForgeServer::new` calls are project-load construction with the field
`None`; all `new_daemon_proxy` calls are in tests). No other production runtime-build
path bypasses this chokepoint.

### C3 — Overlay upsert inside replace_symbol_body (write path, internal)

In `replace_symbol_body` (edit_tools.rs), after the existing
`reindex_after_write(...)` call (edit_tools.rs:789-797) and within the
`if !resolved_target.rerouted` branch (rerouted edits do NOT touch the active
index/overlay, matching the existing reindex guard):

```rust
if !resolved_target.rerouted {
    edit::reindex_after_write(&self.index, &resolved_path, &params.0.path,
                              &new_content, file.language.clone());
    // D15 overlay-WRITER: also upsert the same parsed content into the session
    // overlay so a subsequent single-mode get_symbol reads-your-writes.
    if let Some(ov) = &self.session_working_set {
        // Parse mirrors reindex_after_write (re-read from disk, partial-parse safe).
        let parsed = edit::parse_indexed_for_overlay(&resolved_path, &params.0.path,
                                                     file.language.clone());
        if let Some(parsed) = parsed {
            let mut ws = ov.working_set.write();
            if let Some(entry) = ws.get_mut(&ov.project_id) {   // HASH key
                entry.overlay.upsert(params.0.path.clone(), parsed);
            }
            // entry None: project not in this session's working set; best-effort,
            // base was already updated by reindex_after_write. See stop_conditions.
        }
    }
}
```

`parse_indexed_for_overlay` is a thin reuse of the existing parse in
`reindex_after_write` (edit.rs:416-423): `process_file` + `IndexedFile::from_parse_result`
+ `with_mtime`, returning `Arc<IndexedFile>`. Implementer MAY instead refactor
`reindex_after_write` to return the `Arc<IndexedFile>` it builds and upsert that
(one parse, not two) — preferred if the diff stays inside the two named files plus
`edit.rs`; if it touches `edit.rs` signature, treat `edit.rs` as a 3rd allowed file
and note it. PartialParse yields a valid `IndexedFile` with whatever symbols parsed
(never an error) — see I5 and AC6.

### C4 — get_symbol single-mode reads overlay first (read path, internal)

`get_symbol` single-mode (tools.rs:3569-3573) currently:

```rust
let file = {
    let guard = self.index.read();
    loading_guard!(guard);
    guard.capture_shared_file(&params.0.path)
};
```

Replace with an overlay-first probe (HASH key), falling through unchanged:

```rust
let file = {
    let overlay_hit = self.session_working_set.as_ref().and_then(|ov| {
        let ws = ov.working_set.read();
        let entry = ws.get(&ov.project_id)?;          // HASH key (CRACK fix)
        match entry.overlay.deltas.get(&params.0.path)? {
            crate::live_index::FileDelta::Upsert(f) => Some(Arc::clone(f)),
            crate::live_index::FileDelta::Tombstone => None,
        }
    });
    match overlay_hit {
        Some(f) => Some(f),
        None => {
            let guard = self.index.read();
            loading_guard!(guard);
            guard.capture_shared_file(&params.0.path)
        }
    }
};
```

Reading the delta map directly (not via `IndexView`) is deliberate: a stale overlay
(post-base-swap) means the delta is OLD; the base already has the correct content;
falling through to the base is correct (I4). This avoids a fence/borrow tangle and
keeps the diff to the two named methods.

---

## invariants

- **I1 — lock hierarchy:** the working-set lock is OUTSIDE the daemon
  `bases -> projects -> sessions` hierarchy (daemon.rs session-runtime doc; it is its
  own `Arc<RwLock<WorkingSet>>`). Both C3 (`working_set.write()`) and C4
  (`working_set.read()`) are taken while holding NO daemon-map lock — the edit/read
  methods never hold one. No inversion possible.

- **I2 — overlay write never nested under the index lock:** C4's overlay
  `working_set.read()` is taken and DROPPED (the `and_then` closure returns) BEFORE
  `self.index.read()` on the fallback path. C3 takes `working_set.write()` AFTER
  `reindex_after_write` has returned (its index update is complete). No
  index-lock / ws-lock nesting in either order.

- **I3 — single-project parity for None:** when `session_working_set` is `None`
  (shared instance, local-stdio), both C3 and C4 fall through to today's exact code.
  Byte-identical.

- **I4 — stale overlay is safe:** if a base-swap advances the base such that the
  overlay's fence no longer matches, C4 still returns the delta's `Arc<IndexedFile>`
  (the edited content), which equals what the base now holds. Redundant, never wrong.
  No error path.

- **I5 — upsert carries populated symbols:** the parse pipeline
  (`process_file` + `from_parse_result`) produces a full symbol table; PartialParse
  still yields an `IndexedFile` with the symbols that DID parse. The upserted file is
  symbol-queryable.

- **I6 — base update still happens:** `reindex_after_write` (and thus
  `index.update_file`) still runs for non-rerouted edits BEFORE the overlay upsert.
  Non-overlay readers (batch mode, other tools, other sessions) see correct content
  via the base.

- **I7 / SC-003 — no cross-session leak (the FATAL fix):** `session_working_set` is a
  plain `Option<SessionOverlay>` whose `Arc` is INSIDE the `Option`. `#[derive(Clone)]`
  copies the `Option` by value, giving each `SessionRuntime` clone its OWN slot.
  Setting `Some(...)` on the local clone in `session_runtime()` never mutates
  `project.server` (which stays `None`). Session A's overlay handle is never visible
  to session B. The `WorkingSet` Arc each session carries is session B's own
  (`session.working_set` is per-session), so even the inner handle is not shared.

- **SC-002 — base sharing unaffected:** no `IndexBase` interning happens in C3/C4.
  `WorkingSet::add` (the sharing contract) is untouched.

---

## acceptance_criteria

- **AC1 — PRODUCTION-PATH read-your-writes (false-success guard):** an integration
  test MUST drive the daemon dispatch path `execute_tool_call(runtime, ...)`
  (daemon.rs:3414) with a GENUINELY constructed `SessionRuntime` obtained from a real
  `DaemonState`/session open (so `session_runtime()` wiring at C2 is EXERCISED, not
  hand-set). Sequence: open a project + session, call `execute_tool_call(runtime,
  "replace_symbol_body", ...)` to edit a function, then call
  `execute_tool_call(runtime2, "get_symbol", ...)` for that function in the SAME
  session, and assert the returned body is the EDITED body. The test must FAIL if the
  field is `None` (i.e. if `session_runtime()` did not set it) — prove by asserting
  the overlay delta exists for the path under the session's `working_set` after the
  edit. A test that constructs a bare in-process `SymForgeServer` and hand-sets
  `session_working_set` does NOT satisfy AC1 (it bypasses the wiring under test).

- **AC2 — overlay-served, not just base-served:** the read in AC1 must be proven to
  come from the OVERLAY, not incidentally from the base. Assert
  `working_set.read().get(project_id).overlay.deltas` contains an `Upsert` for the
  edited path. (Today the base also reflects the edit via `reindex_after_write`; AC2
  proves the overlay branch specifically is live, so the D15 read flip has real
  coverage.)

- **AC3 — unedited file unchanged:** `get_symbol` for a file NOT edited this session
  returns the same result as today (overlay miss → base fall-through). No regression.

- **AC4 — local-stdio parity:** with `session_working_set == None`, `get_symbol` and
  `replace_symbol_body` are byte-identical to today.

- **AC5 — no cross-session leak:** open TWO sessions on the same project; edit in
  session A; assert `get_symbol` in session B does NOT see session A's overlay edit
  (B sees the base content, which is updated only via the shared index — but the
  OVERLAY delta must not appear in B's working_set). This directly exercises SC-003 /
  the FATAL fix.

- **AC6 — partial-parse case (Attack 2 nit):** edit a symbol so the resulting file
  has a syntactically broken region elsewhere (PartialParse). Assert the overlay
  upsert still happens and `get_symbol` for a SUCCESSFULLY-parsed symbol in that file
  returns the edited body (the upsert must not be skipped just because the file
  partially failed to parse). The test must not lie by only ever feeding
  clean-parsing input.

- **AC7 — lock order:** no `bases`/`projects`/`sessions` guard is held when C3's
  `working_set.write()` or C4's `working_set.read()` is taken. (Reviewed + asserted
  by construction; covered by the suite running clean under `--test-threads=1`.)

- **AC8 — gates green:** `cargo fmt --check`, `cargo check`,
  `cargo clippy --all-targets -- -D warnings`,
  `cargo test --all-targets -- --test-threads=1`, `cargo build --release` all pass.

---

## evidence_required

- New integration test (in `src/daemon.rs` tests, alongside the existing
  `new_daemon_proxy`/session tests near daemon.rs:6945+, since that scope already
  constructs real `DaemonState` + sessions) satisfying AC1/AC2/AC5 by driving
  `execute_tool_call` with a real `SessionRuntime`.
- A `replace_symbol_body`/`get_symbol` overlay unit or integration test for AC6
  (partial parse) and AC3/AC4.
- `cargo test --all-targets -- --test-threads=1` output showing the new tests pass.
- `cargo clippy --all-targets -- -D warnings` clean output.
- A note in the test or PR confirming the field was observed `Some` on the
  session-runtime clone and `None` on the shared instance (proves C2 wiring live).

---

## stop_conditions

- STOP and surface if `WorkingSet::get_mut(&project_id)` returns `None` for the
  active session's hash id during the upsert — this would mean the session's working
  set was never seeded with the active project (a wiring bug). The upsert is
  best-effort (base already updated by `reindex_after_write`), but a persistent
  `None` here means AC1/AC2 cannot pass — investigate the seed path
  (daemon.rs:855/1239), do not paper over it.
- STOP if implementing the bridge-fix requires touching MORE than the 2 named source
  files (`mod.rs`, `daemon.rs`) plus the 2 method bodies (`edit_tools.rs`, `tools.rs`)
  and at most `edit.rs` for the parse-reuse refactor. Any wider blast radius means
  Option 3 is not holding — surface and get approval before expanding.
- STOP if `get_symbol` cannot read the overlay without taking the index lock first
  (would break I2) — re-examine the borrow before proceeding.
- STOP if any second production `project.server.clone()` or runtime-build path is
  found that bypasses `session_runtime()` (would produce a `None` overlay and silently
  skip read-your-writes — a false-success). None exists today; re-verify if the build
  changes.

---

## verification_command

```
cd E:\project\symforge-012
cargo fmt --check
cargo check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets -- --test-threads=1
cargo build --release
```

Run with `CARGO_INCREMENTAL=0`. All must pass before reporting done.
