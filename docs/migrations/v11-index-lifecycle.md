# V10 → V11: the index-lifecycle migration (Feature 020 Slice 4)

**Audience**: embedders of the `symforge` crate and operators of the server
binary. **Release**: the activation cut merges as one indivisible unit and
release-please mints **11.0.0** — the one planned MAJOR break the Feature
020 campaign froze. The authority for every name below is the frozen
contract `specs/020-repository-knowledge-index/contracts/public-api-v11.json`
(244 inventoried V10 members with per-category dispositions); this document
is the narrative, the contract is the law.

## 1. The migration boundary in one paragraph

V10 exposed the raw engine: crate-root modules (`live_index`, `parsing`,
`discovery`, `domain`, ...), a shared mutable index handle, authorityless
search, per-file mutation, and snapshot loaders. V11 retires that surface
entirely and replaces it with two doors, each carrying typed authority:

- **`symforge::embed`** (feature `embed`, no default features) — an
  embedder opens ONE [`EmbeddedSourceHandle`] through
  [`ProcessIndexRuntime`] and works with typed claims, receipts, and
  refusals. Nothing else is public in the embed cell.
- **`symforge::server_api`** (feature `server`) — the whole CLI/daemon/MCP
  server behind one entry: `run(args) -> Result<ServerExit,
  ServerBootstrapError>`. The binary is a shim over this door.

Everything the lifecycle knows — supervisor, candidate pipeline, observer,
verification, query leases — sits BEHIND these doors. There is no raw
bypass: a result you did not obtain through a handle's typed operation is a
result V11 will not give you.

## 2. Removed exports (what breaks when you bump)

Category-level summary of the frozen dispositions; the full 244-member
inventory with per-member rationale is in `public-api-v11.json` and
`contracts/v10-authority-retirement-v11.md`.

| Removed (V10) | Category disposition |
|---|---|
| `symforge::live_index::*` (`LiveIndex::load`, `SharedIndex`, `SharedIndexHandle`, `update_file_from_disk`, `remove_file`, snapshot loaders, ...) | Raw crate-root modules retired; state and mutation now live behind the handle. |
| `symforge::parsing`, `::domain`, `::discovery`, `::git` (`GitRepo`), `::hash`, `::paths`, ... | Raw crate-root modules retired (internal `pub(crate)` mounts remain for the engine itself). |
| Authorityless search entry points | Replaced by `SymbolSearchRequest`/`TextSearchRequest` on the handle, returning `Claim`-wrapped results with `ClaimProvenance`. |
| Raw per-file mutation and reload methods | Replaced by handle operations that consume a `SourceMutationPermit` internally and return `OperationReceipt`; refusals are typed `SourceRefusal` values, never silent no-ops. |
| V10 snapshot loaders | Replaced by the V11 untrusted-seed restore (§4). |
| STEL ledger and deep module re-exports from `embed` | Retired from the embed surface. |

The only V10 items that survive verbatim on the embed surface are the
engine identity (`EngineInfo` / `engine_info`) — one call, no I/O.

## 3. Replacement API (the V11 embed atoms)

```toml
symforge = { version = "11", default-features = false, features = ["embed"] }
```

```rust
use symforge::embed::{ProcessIndexRuntime, EmbeddedSourceSpec};

let runtime = ProcessIndexRuntime::acquire()?;            // typed SourceRefusal on failure
let handle = runtime.open_embedded_source(
    EmbeddedSourceSpec::current_worktree(root_path))?;    // ONE handle per source
```

The full atom set (pinned by `src/embed.rs`'s compile-time contract test
`facade_contract_is_stable`): `ProcessIndexRuntime`, `EmbeddedSourceSpec`,
`EmbeddedSourceHandle`, `SourceRuntimeView`/`SourceRuntimePhase`,
`SymbolSearchRequest`/`SymbolSearchResult`/`SymbolMatch`,
`TextSearchRequest`/`TextSearchResult`/`TextMatch`, `Claim`,
`ClaimProvenance`, `EvaluationProvenance`, `AtomicAuthority`,
`OperationKind`, `OperationReceipt`, `RefreshTicket`, `ReceiptWaitError`,
`RetryAdvice`, `SourceRefusal`/`SourceRefusalKind`, `SourceCloseReceipt`/
`SourceCloseReport`, `ShutdownReceipt`/`ShutdownReport`, `EngineInfo`/
`engine_info`.

Mapping the common V10 embedder moves:

| You used to... | Now you... |
|---|---|
| `LiveIndex::load(root)` and hold a `SharedIndex` | `ProcessIndexRuntime::acquire()` + `open_embedded_source(spec)` — the handle owns the state. |
| `update_file_from_disk(&shared, root, rel)` after an edit | Request a refresh through the handle (`RefreshTicket` lane); completion arrives as a receipt, staleness as typed `RetryAdvice`, never a silent stale serve. |
| `remove_file(&shared, rel)` | The observer lane confirms absence itself; deletions are observations, not caller commands. |
| Search the index directly | Submit a typed search request; the result is a `Claim` carrying provenance you can audit. |
| Load a snapshot to skip cold start | Nothing to call: restore is engine-internal. A pre-existing snapshot is an UNTRUSTED SEED — it may accelerate re-proof, never confer authority (§4). |
| Match on error strings | Match on `SourceRefusalKind` — refusals are a closed, typed set. |

Server consumers: replace any direct module reach-through with
`symforge::server_api::run(argv)`; the exit is the closed enum
`ServerExit { RefusedToStart, Success }` and bootstrap failure is the
opaque `ServerBootstrapError` (renders its full cause chain via `Display`;
deliberately not matchable, so new refusal causes are non-breaking).

## 4. Snapshot migration and rollback constraints

Design (frozen 020:T065; semantics oracle-pinned in
`tests/snapshot_v11_migration.rs`):

- Every pre-existing V10 snapshot/cache byte at restart is an **untrusted
  seed**: bounded pre-decode capacity, digest verification, and complete
  current-process re-observation before ANYTHING promotes to `Current`. A
  seed that fails proves nothing and quarantines with its original bytes
  preserved.
- The V11 state lives under the `.symforge/v11/` namespace, isolated from
  the V10 namespace; concurrent V10 writers cannot race the restore into
  mixed authority.
- **Rollback is preserved by construction**: the V10 namespace is never
  destroyed or rewritten by V11 restore. Rolling back to a 10.x binary
  finds its own store untouched; the worst cost is a cold re-index of work
  done only under V11.
- Secret-canary bytes never enter snapshots, quarantine metadata,
  receipts, or diagnostics (FR-012).
- Team-artifact persistence is `ProjectStateDir`-only and carries no
  source-mutation authority; git-visibility writes follow the exact
  frozen FR-051 four-state receipt/refusal matrix (`already_tracked`,
  `untracked_visible`, `ignored_force_add_required`,
  `git_visibility_unavailable`).

> **as_of 2026-08-20 — narrowed during T038 round 1**: the on-disk
> `CURRENT_VERSION` bump (7 → 8) and the format-version seed-preservation
> gate are now LIVE in `src/live_index/persist.rs`: a prior-format (V10)
> snapshot never restores, its original bytes stay in place for rollback,
> and a copy is quarantined under `.symforge/v11/quarantine/
> index-snapshots/` (oracle `a_v10_format_snapshot_is_a_preserved_seed_
> never_authority`, `src/live_index/persist.rs`). What remains UNWIRED:
> the richer per-entry proof machinery in `src/index_lifecycle/
> snapshot.rs` (`SnapshotStore` — pre-decode capacity rejection,
> entry-by-entry re-proof, the FR-051 four-state git-visibility export
> receipt) is still a standalone dark module with no production caller
> from `persist.rs`; today's live gate is coarser (whole-snapshot
> format-version admission, not per-entry proof). See
> `docs/reviews/FEATURE-020-SLICE4-ACTIVATION-EVIDENCE-v11.md` §7a for the
> full record. Wiring `SnapshotStore` itself into the live restore path
> remains open.

Rollback constraints beyond snapshots:

- **The cut is indivisible (FR-001)**: there is no partial rollback of the
  lifecycle. Reverting means reverting the entire cut merge (one squash
  commit on `main`) and releasing a subsequent major; no configuration,
  environment variable, or feature flag re-enables V10 behavior inside an
  11.x binary — the activation mode machine is process-wide and
  non-configurable.
- **Downgrade path**: install a 10.x binary. Its store is intact (above).
  `.symforge/v11/` contents are simply ignored by 10.x.
- **MCP clients** are unaffected by the crate-surface break: the tool
  surface (39 advertised / 40 registered, compact-3 opt-in) is unchanged
  by this migration; V11 changes what the answers are backed by, not the
  wire protocol.

## 5. Compile-fix crib sheet

- `error[E0432]: unresolved import symforge::live_index` (or `parsing`,
  `domain`, ...) — you were on the raw V10 surface; move to
  `symforge::embed` (§3).
- `EmbeddedSourceHandle`/`ProcessIndexRuntime` not found — enable
  `features = ["embed"]` with `default-features = false`.
- `symforge::server_api` not found — it requires `feature = "server"`
  (the default); it is deliberately absent from embed builds.
- Exhaustive matches on V10 enums that gained typed replacements — the
  V11 sets are the contract's; match the new types, and note
  `ServerBootstrapError` is intentionally opaque.
