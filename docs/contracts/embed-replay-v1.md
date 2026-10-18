# Embedded replay v1

`symforge::embed::parity::replay` is the supported durable replay lane for a
trusted host. It is project scoped and host scoped. The host supplies a
canonicalizable project root, its **actual durable project state directory**,
and a stable nonempty scope identifier for the room or source authority. Each
scope has a private database beneath the project's replay directory, bound to
that root, scope, and physical source/state directories. Multiple authorized
rooms can share the project state directory without sharing replay keys. An
existing root-level v1 database remains assigned to its original scope; old
JSON reservations are checked across every scope. Memory-only state cannot
support replay admission.

The host constructs a nonempty `ReplayKey` and a `RequestFingerprint::for_json`
from an operation name and canonical arguments. Requests must bind every
mutation precondition and hash any replacement or response payload, rather than
putting source bytes in the request. Raw keys are never logged or stored. The
key hash and request hash use the prior MCP framing, so an old reservation
cannot be silently rebound under a different hash. Debug output redacts keys.

`reserve_with_lease` is the supported admission path. It performs one SQLite
`BEGIN IMMEDIATE` transaction. Its unique key row and random owner token are
visible together or not at all. `Acquired` returns the sole `OwnerLease`;
`Existing` returns the current typed record. A different request fingerprint
for the same key is a conflict. The deprecated `reserve` path still exists but
writes no `lease_expires_at`; those `not_started` rows never expire and need
operator action after a crash. The database uses a five second lock wait and
full synchronous writes. Lock contention past that wait is `ReplayError::Busy`
(retryable); other I/O or database errors fail the operation. The host must
not execute on failure.

The state machine is:

```text
absent -> NotStarted -> Started -> Completed
                    \           \-> Failed
                     \-> Uncertain -> Completed or Failed (reconciliation only)
NotStarted -> absent (same owner only)
```

The host calls `mark_started` (or `mark_started_with_lease`) immediately before
the external effect. A leased reservation that has already expired cannot
commit `mark_started` (`StaleOwner`); the row stays `not_started` for takeover.
`mark_started_with_lease` arms a fresh execution lease on the `started` row.
Only the lease owner can release `NotStarted`, and only while it is still
`NotStarted`.
Once started, a crash leaves `Started`, even after process restart or any amount
of time. An ambiguous effect becomes `Uncertain`. Neither state permits a new
execution. `reconcile` accepts only `Started` or `Uncertain` and records a digest
of a trusted host's observed postcondition, with a generation compare and swap.
The observed bytes must hash to the same post-image digest as the proposed
outcome. The host must read those bytes from the current authoritative target
and establish that the old owner has exited before reconciling `Started`.
It can classify the effect as terminal; it cannot reopen the execution. A
stale owner cannot overwrite the reconciliation. A terminal `Failed` must mean
a known terminal failure, never an ambiguous write.

`ReplayOutcome` stores only a fixed outcome kind, a response digest, and a
post-image digest. It never stores a response body, source text, error message,
or unrestricted path. `from_response_and_post_image` secret-scans both buffers
and caps the response at 64 KiB. `from_embedder_digest` records an embedder-
supplied length and lowercase SHA-256 without reading response bytes: the
embedder owns the response secret scan, and there is no store-side response
size cap on that path. SymForge still secret-scans and size-caps the post-image
and evidence payloads. A completed mutation is safe to answer as replayed only
after the host rereads the target and calls `matches_post_image` with the
current bytes. A mismatch is a refusal requiring investigation, never an
automatic retry. This database is an execution guard, not a response cache.

The old JSON store at `<project state>/idempotency/records` is a migration gate.
If it exists, new reservations refuse until the host has stopped all old replay
writers and recorded `LegacyCutoverEvidence` from its cutover checkpoint. Old
per-key directories always refuse with `LegacyStateRequiresReconciliation`,
including partial and corrupt reservations. This API never reads response text,
deletes, rewrites, or auto-adopts old JSON bytes. The operator must preserve
those bytes and reconcile them through the old-state recovery workflow before
retiring that key. Concurrent old and new writers are outside this contract:
the host must enforce exclusive cutover before calling the cutover method.

The project state directory is created privately where the platform supports
mode bits, and existing symlinks at the replay and scope directory/database
paths are refused. A user-local state placement is supported; callers must pass
the same placement used by the old replay store. The host owns directory access control
and process lifecycle enforcement on platforms whose filesystem permissions
cannot be set through mode bits.
