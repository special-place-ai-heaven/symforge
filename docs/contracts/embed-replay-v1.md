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

`reserve` performs one SQLite `BEGIN IMMEDIATE` transaction. Its unique key row
and random owner token are visible together or not at all. `Acquired` returns
the sole `OwnerLease`; `Existing` returns the current typed record. A different
request fingerprint for the same key is a conflict. The database uses a five
second lock wait and full synchronous writes. Busy or I/O errors fail the
operation; the host must not execute on failure.

The state machine is:

```text
absent -> NotStarted -> Started -> Completed
                    \           \-> Failed
                     \-> Uncertain -> Completed or Failed (reconciliation only)
NotStarted -> absent (same owner only)
```

The host calls `mark_started` immediately before the external effect. Only the
lease owner can release `NotStarted`, and only while it is still `NotStarted`.
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
or unrestricted path. Outcome and evidence construction checks the existing
secret detector and refuses sensitive or indeterminate payloads; request,
response, and evidence sizes are bounded. A completed mutation is safe to answer as replayed only
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
