# Feature Specification: Engine-First Multi-Project Index Primitive + Per-Connection Retarget

**Feature Branch**: `012-harness-agnostic-mcp`

**Created**: 2026-06-19

**Status**: Draft (engine-first; supersedes the earlier multi-tenant-server-platform draft of this spec)

## Pivot decision & rationale (2026-06-19)

This spec was originally drafted as a **multi-tenant MCP server platform** (per-tenant server, owned admin dashboard = spec 013, owned durable tenancy/economics DB = spec 014). A requested adversarial review by three independent reviewers (a tech-researcher who read AAP's source + external precedent, an architecture strategist, and a red-team) converged on a different direction, and the project owner chose **engine-first**. The decision and why it must not be lost:

- **SymForge's primary consumer, AAP, does not use the MCP server at all** — it embeds the engine in-process via the `embed` feature (`aap-mcp` has zero symforge dependency; verified in AAP's tree). AAP already owns isolation (Firecracker microVMs + git worktrees), persistence (SingleStore), UI (React SPA + admin), and auth. A SymForge-owned multi-tenant server + dashboard + DB would **duplicate AAP** and grow the very MCP/STEL surface a prior audit found overstated.
- **The valuable capability is an engine primitive, not a server platform.** The base+overlay multi-project index generalizes a primitive the engine already ships (`ArcSwap<LiveIndex>` snapshot in `live_index/store.rs`). Field precedent is decisive: rust-analyzer/salsa keep the snapshot/fork in the **library** and put multiplexing in a **thin separate shim** (`ra-multiplex`); DuckDB/Lucene the same (engine embedded, host owns tenancy).
- **The field actually reported two bugs**, not a multi-tenant need: wrong-repo binding (one daemon pinned to launch CWD; `path:` does not retarget) and the `path:`-as-project-selector vocabulary trap. The owner separately requires the **multi-project / cross-project** capability ("index several projects at once, compare / find similar snippets") — which the **engine primitive** delivers. Many-harnesses-sharing-one-process-as-isolated-tenants is **not** evidenced (harnesses each spawn their own process today) and is **deferred** until demand exists.

**Resulting shape (this spec):** build the base+overlay multi-project index as a **library primitive**; fix the real wrong-repo bug with **per-connection retarget + bound-root visibility + corrected `path:` vocabulary + a glossary**, reusing the daemon's existing multi-project registry; expose the server multiplexing as a **thin shim** over the primitive. Companion specs 013/014 are **shrunk** (see below) and the multi-tenant-platform framing is dropped.

**First action (de-risk before any platform code):** a spike generalizing `ArcSwap<LiveIndex>` into a base+overlay `IndexView` and proving two simulated consumers share one immutable base with isolated overlays and zero cross-leakage. If clean, the primitive is justified; if it hits a perf cliff on base-commit advance (the named CoW risk), revisit.

## Context

Three independent harnesses (Cursor, Mistral, AAP-collaborator) found: SymForge binds a connection to one project from the launching process's working directory, before the handshake, and freezes it — producing confident, silent **wrong-repository** answers after a workspace switch; the within-project filter `path:` looked like a project selector but could not retarget; harnesses spawned extra server processes per repo (port contention, stray windows). Separately, the owner needs a single harness to work across **several projects at once** (cross-project search/comparison).

The engine already has the bones: the daemon is a **multi-project registry** (`projects: HashMap<project_id, ProjectInstance>`, per-session routing), and `live_index` has a cheap immutable snapshot (`ArcSwap<LiveIndex>`). This feature builds the multi-project index **primitive** on those bones and fixes the binding/vocabulary bugs — without turning SymForge into a multi-tenant application platform.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - One consumer, several projects at once, cross-project work (Priority: P1)

A consumer (a harness over the shim, or an in-process embedder) holds repositories A, B, C indexed at once and queries across them — "find functions in B and C similar to this snippet from A," "where across my open projects does this pattern appear" — getting results attributed to each source project.

**Why this priority**: The owner-stated capability and the reason for the multi-project index primitive.

**Independent Test**: Open three repositories in one working set; run a cross-project search; confirm correctly source-attributed matches from each.

**Acceptance Scenarios**:
1. **Given** a working set {A,B,C}, **When** a cross-project search runs, **Then** results from all targeted projects return, labeled by source project.
2. **Given** several projects open, **When** a query is scoped to a subset or a single project, **Then** only those are searched.

### User Story 2 - Retarget the binding; never answer the wrong repo silently (Priority: P1)

A consumer that was bound to repo A retargets to repo B (declares new workspace roots, or names the project explicitly). Subsequent queries answer about B. The bound root is visible in every response, so a stale/wrong binding is immediately detectable rather than silently wrong.

**Why this priority**: This is the actual field bug (C4). It is the highest-value fix and is cheap on the existing daemon registry.

**Independent Test**: Bind to A, retarget to B, confirm status and results reflect B; confirm every response shows the bound `project_root`.

**Acceptance Scenarios**:
1. **Given** a binding to A, **When** the consumer retargets to B (roots change or explicit `project:`/`root:`), **Then** the next query and status reflect B.
2. **Given** any response, **When** it is returned, **Then** it includes the bound `project_root` (and index readiness) so mis-binding is loud, not silent.
3. **Given** a query whose `path:` is outside the bound project, **When** received, **Then** a clear "path outside project" error returns — `path:` is never reinterpreted as a project selector.

### User Story 3 - Shared base, isolated overlay, no lockouts (Priority: P1)

Two consumers on the same canonical root at the same commit share one immutable base index; each has its own copy-on-write overlay for dirty/uncommitted deltas, invisible to the other. One consumer's reindex/switch/edits never reload, invalidate, or lock out the other or the user's editor.

**Why this priority**: Delivers the cross-project capability at base+epsilon memory (not N copies) and kills the lock/reload storm observed in the field.

**Independent Test**: Two consumers on the same canonical root; one makes uncommitted edits; confirm the other sees only the clean base; confirm an external editor save is never blocked.

**Acceptance Scenarios**:
1. **Given** two consumers on canonical root R at the same commit, **When** one makes uncommitted edits, **Then** the other's results reflect the base only (overlay never cross-visible).
2. **Given** one consumer reindexes/switches, **When** another reads the shared base, **Then** the reader is not reloaded, invalidated, or blocked beyond a bounded brief interval.
3. **Given** two worktrees of the same logical repo (different canonical roots), **When** both are open, **Then** they get separate base layers (no cross-state contamination).

### User Story 4 - Honest errors and a self-documenting surface (Priority: P2)

Unsatisfiable requests return clear, specific errors; the surface explains its own vocabulary, especially project-target vs within-project filter, and that economy figures are estimates.

**Independent Test**: Issue a query missing the required question, one targeting a project not open, and an edit matching current on-disk bytes; fetch the glossary; confirm clear errors, no false edit rejection, and documented target-vs-filter semantics.

**Acceptance Scenarios**:
1. **Given** a request omitting the required question field, **When** received, **Then** a clear "a question is required" validation error returns (not an opaque internal/deserialize error).
2. **Given** a guarded edit whose `if_match` equals the current on-disk bytes, **When** applied, **Then** it is NOT falsely rejected due to index-vs-disk formatting (observed false-reject bug); the check compares normalized on-disk bytes.
3. **Given** the surface, **When** a caller reads parameter descriptions or fetches the legend resource, **Then** project-target vs within-project filter, the question field, intents, and the estimated nature of economy figures are explained.

### User Story 5 - Thin shim, one server, no per-repo processes (Priority: P2)

Standalone harnesses attach to a single running server (a thin multiplexing shim over the engine primitive). Working across repositories never spawns extra server processes, terminal windows, or competing listeners.

**Independent Test**: Attach harnesses for several repositories to one server; confirm one process serves them, no extra listeners/windows.

**Acceptance Scenarios**:
1. **Given** a running server, **When** a consumer needs another repository, **Then** the same server serves it via retarget/new binding, not a spawned process.
2. **Given** normal use, **When** harnesses operate, **Then** no foreground/operator window appears as a side effect.

### Edge Cases

- Same canonical root + same commit + clean tree: 100% shared base; overlays private.
- Different worktrees of one logical repo: separate base layers keyed by canonical root.
- A consumer with uncommitted edits: overlay reflects them; base advance re-derives/invalidates overlay per documented rules.
- Reconnect: a consumer that declares a stable identity is rebound to its working set (lighter than full tenant identity — continuity, not a security boundary).
- Server restart: `(consumer, project)` bases rehydrate lazily on first access (no thundering herd), driven by minimal persisted registry metadata.
- Query targets a project not in the working set: clear "project not open" error.

## Requirements *(mandatory)*

### Functional Requirements — Engine primitive (the core)

- **FR-001**: The engine MUST provide a base+overlay index primitive: an immutable base index keyed by `(canonical root, commit)`, plus a per-consumer copy-on-write overlay holding only dirty/uncommitted deltas. (Generalizes the existing `ArcSwap<LiveIndex>` snapshot.)
- **FR-002**: The base index MUST be shared read-only across consumers on the same `(canonical root, commit)`; the engine MUST NOT duplicate immutable repository facts per consumer.
- **FR-003**: Overlays MUST be isolated: a consumer's overlay (working-set deltas, dirty edits) MUST NOT be visible to another consumer.
- **FR-004**: The primitive MUST support a working set of multiple projects per consumer and cross-project operations (search, similarity/match comparison) with results attributed to their source project.
- **FR-005**: The primitive MUST live in the engine (`live_index`) so BOTH an in-process embedder (e.g. AAP) and the MCP shim can drive it. It MUST be exposed deliberately, versioned so as not to weld volatile overlay-invalidation internals into the frozen `embed` semver facade (expose via engine modules embedders opt into; keep the facade contract minimal).

### Functional Requirements — Binding / retarget (the field fix)

- **FR-006**: A connection MUST be able to retarget its bound project at any time — by declared workspace roots changing, or by an explicit `project:`/`root:` mechanism — reusing the daemon's existing multi-project registry; no reconnect required.
- **FR-007**: Initial binding MUST derive from the workspace the consumer declares at connect time (precedence: explicit override > declared roots > defined default), not the server process's working directory.
- **FR-008**: Every `symforge`/`status` response MUST surface the bound `project_root` (and index readiness), so a stale/wrong binding is loud, not silent.
- **FR-009**: `path:` MUST remain a within-project filter; project selection MUST be a distinct explicit mechanism; a `path:` outside the bound project MUST return a clear error, not empty/misleading results.
- **FR-010**: The physical file-watch/read layer MUST be keyed by declared canonical root (same root shares one watcher; different roots get separate layers); indexing MUST NOT hold locks that block other consumers or the user's editor.

### Functional Requirements — Surface honesty

- **FR-011**: Omitting the required question/query field MUST return a clear validation error, not an opaque deserialize error.
- **FR-012**: The guarded-apply `if_match` check MUST compare against normalized on-disk bytes so a valid edit matching the current file is NOT falsely rejected on index-vs-disk formatting differences.
- **FR-013**: The surface MUST provide in-place parameter descriptions (project target, within-project filter, question, intent, symbol) and a fetchable legend/glossary resource defining vocabulary (project target vs within-project filter; intents; server vs background service vs install; base+overlay model; that economy figures are estimates; status fields).

### Functional Requirements — Thin shim

- **FR-014**: The MCP server MUST multiplex connections as a thin shim over the engine primitive and the daemon registry; normal multi-repository use MUST NOT spawn additional server processes, listeners, or terminal windows.
- **FR-015**: The system MUST maintain a minimal registry (consumer/connection -> working set; lazy-rehydrate metadata) sufficient for FR-006 retarget and restart recovery, with a seam for the shrunk persistence (014). On restart, `(consumer, project)` bases MUST rehydrate lazily on first access.
- **FR-016**: The existing single-harness flow MUST continue to work without regression.

### Shrunk companion specs (re-scoped by the engine-first pivot)

- **Spec 013 (SHRUNK)**: NOT a SymForge-owned multi-tenant dashboard. Instead, expose tenancy/telemetry as a **read-only JSON API** (the existing `/api/v1` DTO pattern) that a host (AAP) renders in its own UI, and keep only the existing minimal `/admin` for standalone operators. Do not build a competing operator console.
- **Spec 014 (SHRUNK)**: NOT a SymForge-owned durable tenancy+economics platform. Instead, a **minimal durable registry** (connection/working-set + lazy-rehydrate metadata for FR-015), reusing the SQLite already in-tree. The per-tenant **economics** ledger is deferred (DEF-001) and stays in the existing STEL store; do NOT persist/per-tenant-scale the economy figures until they are grounded (audit C1-B). Code index stays in-memory.
- **DEF-001 (deferred)**: Per-connection economy-accounting isolation, AND grounding the economy predictor (C1-B) before any durable economics. Lives in the existing STEL store until then.

### Key Entities

- **Immutable Base Index**: Read-only parsed repository facts keyed by `(canonical root, commit)`; shared across consumers on that state. Generalizes `ArcSwap<LiveIndex>`.
- **Overlay**: A consumer's copy-on-write dirty/uncommitted deltas over a base; never cross-visible.
- **Working Set**: A consumer's set of (base + overlay) projects; supports add/remove/switch and cross-project query.
- **Canonical Root**: Declared workspace root; keys the base index and the shared file-watch layer.
- **Thin Shim**: The MCP server multiplexing connections over the primitive + daemon registry.
- **Glossary Resource**: Fetchable surface-vocabulary reference.

## Success Criteria *(mandatory)*

- **SC-001**: A consumer holds >=3 projects indexed concurrently and runs a cross-project search returning correctly source-attributed matches from each, in 100% of trials.
- **SC-002**: Two consumers on the same `(canonical root, commit)` share one immutable base; measured memory is ~base+epsilon, not 2x a full index.
- **SC-003**: One consumer's overlay edits are never visible to another (0 cross-leakage); one consumer's reindex/switch causes 0 forced reloads/interruptions for another on the same base.
- **SC-004**: After retarget, the next query/status reflect the new project in 100% of trials; every response shows the bound `project_root`; 0 silent wrong-repo answers.
- **SC-005**: While two consumers index the same canonical root, an external editor save under that root is never blocked by SymForge.
- **SC-006**: A missing question, a project-not-open target, and a valid edit matching on-disk bytes each behave correctly (clear errors for the first two; no false edit rejection) in 100% of trials.
- **SC-007**: The glossary resource is retrievable and parameter descriptions explain project-target vs within-project filter; a reviewer can state from the surface alone that `path:` is within-project only and economy figures are estimates.
- **SC-008**: One server process serves N repositories with 0 extra listeners and 0 stray console windows.
- **SC-009**: On server restart, no all-at-once rebuild; `(consumer, project)` bases rehydrate lazily on first access.
- **SC-010**: The existing single-harness flow passes its current behavior checks unchanged (no regression).
- **SC-011**: The `embed` semver facade contract is not broken; the primitive is exposed without welding volatile overlay internals into the frozen contract.

## Assumptions

- The engine already provides the multi-project registry (daemon) and a cheap immutable snapshot (`ArcSwap<LiveIndex>`); this feature generalizes the snapshot into base+overlay and reuses the registry — it does not introduce a new multi-project store.
- The code index stays in-memory (base rebuilt from source, watcher-fed); only minimal registry/rehydrate metadata persists (shrunk 014).
- AAP keeps consuming the engine via `embed` and owns its own tenancy/persistence/UI/isolation; this feature must NOT break or bypass the embed facade.
- Multi-harness-shared-process isolation as a *security boundary*, an owned multi-tenant dashboard, and durable per-tenant economics are **deferred** until demand is evidenced (no field/owner request to date).
- "No regression" is measured against current single-harness behavior.

## Risks

- **CoW overlay invalidation correctness** (commit advance, on-disk change re-derivation) is the load-bearing complexity; must be specified in planning and validated by the spike. (Named the key risk by all reviewers.)
- **Embed-facade versioning**: exposing the primitive must not weld volatile internals into the frozen semver facade (would force MAJOR bumps + lockstep AAP upgrades on every invalidation fix).
- **Transport**: the `/mcp` HTTP path is a documented stateless single-index singleton today; the thin shim's per-connection binding/retarget must be confirmed feasible (linchpin), though the existing daemon session model already routes per session.
- **Spike outcome**: if `ArcSwap`→base+overlay hits a perf cliff on base-commit advance, the primitive design must be revisited before the shim/retarget work proceeds.

## Dependencies

- De-risk spike: generalize `ArcSwap<LiveIndex>` → base+overlay `IndexView`; prove two consumers share one base with isolated overlays (SC-002/003) before further build.
- Reuses the daemon's existing multi-project registry and per-session routing for retarget (FR-006).
- Shrunk 013 (read-only JSON API) and 014 (minimal registry) depend on this spec's primitive + registry seam.
