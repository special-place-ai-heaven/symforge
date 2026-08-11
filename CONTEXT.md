# SymForge Domain Context

This file names the domain concepts used by SymForge architecture and design
work. These names describe product behavior and invariants; implementation type
names may differ while a design is still under review.

## Project index lifecycle

**Project slot** — The stable per-project ownership record. A slot may exist while
no index is queryable. Sessions join the slot rather than constructing independent
loads for the same canonical project. A stopping slot remains registered as a
tombstone until observers and publication-capable workers are quiescent. Immutable
retired generations may outlive the slot under independent capacity ownership.

**Slot instance ID** — A process-unique, never-reused identity for one project-slot
incarnation. Close/reopen cannot make stale work authoritative by recreating the
same logical project ID.

**Source slot** — The per-source lifecycle record within a project slot. The current
worktree and each admitted local-ref source have independent active generations and
work state.

**Candidate generation** — A complete index build in progress. It is isolated from
queries, caches, checkpoints, and every committed-state mutation until promotion.

**Verified generation** — An immutable, source-bound generation whose manifest,
identity, observation watermark, and capability completeness have passed the
promotion checks.

**Active generation** — The verified generation retained in the source slot's active
pointer. A failed build never changes this pointer. Strict queryability additionally
requires `WorkState::Idle` and the complete strict-current scope proof.

**Binding epoch** — An identity token tying project ID, canonical root, source
identity, and generation authority together. Mutation authority is validated as
one token; a generation counter and root are never sampled independently.

**Physical root identity** — Platform evidence, obtained from an open directory
handle where supported, that the directory object behind a canonical path has not
been replaced. Inability to prove continuity invalidates promotion conservatively.

**Capacity lease** — A process-wide reservation owned by the allocation or immutable
generation it accounts for. It covers active, candidate, retired-but-pinned,
derived, scratch, and bounded watcher-journal residency and releases only when the
actual memory owner is dropped.

**Watcher journal** — The bounded process-local record of source-change hints
captured while a candidate is built. A detected gap, overflow, disconnect, or
eviction invalidates catch-up and requires a new authoritative observation.

**Observation cursor** — A watcher-registration epoch plus a monotonic sequence.
Cursors from different registrations are incomparable.

**Promotion** — The single atomic commit that makes a candidate the active
generation and acknowledges the watcher watermark through which it was verified.

**Runtime snapshot** — One immutable publication containing the source slots' active
generation references, work state, observation cursors, and runtime epoch.

**Query operational snapshot** — An immutable, versioned per-request view of mutable
non-source evidence such as session/persistent frecency, with one evaluation time.
It may affect ordering only; it cannot establish source truth, readiness, or absence.

**Query lease** — A captured runtime snapshot, its selected verified generation, and
one query operational snapshot. Source content, identity, availability, and health
come from the runtime snapshot; ranking uses only the attached operational snapshot.

**Strict-current scope contract** — The versioned, closed set of source-derived
facts that must be terminal and complete before a generation can promote. An
advertised surface cannot silently accept truncated, unreadable, partially parsed,
or unknown coverage.

**Source mutation intent** — A lifecycle operation that validates the complete
binding token and atomically marks the source Dirty before SymForge performs its
first repository disk side effect. A committed write returns to Idle only through
verified candidate promotion.

**Last-known-good generation** — The previous active generation retained unchanged
while replacement work runs or fails. It is not silently relabeled current.

**Source index lifecycle module** — The lifecycle-owning module behind each source
slot. It owns single-flight loading, candidate construction, watcher catch-up,
retries, promotion, query leases, and health projection. Project membership remains
in the project registry and process-wide resource admission remains in the capacity
pool.
