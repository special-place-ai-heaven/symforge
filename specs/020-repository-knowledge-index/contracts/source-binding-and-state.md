# Contract: Source Binding and Runtime State

**Status**: Frozen (2026-07-17)<br>
**Surface**: startup/session binding, `index_folder`, `health`, `status`, init,
checkpoint/snapshot/idempotency consumers<br>
**Core invariant**: permission to index a source is not permission to write state
inside that source

## Outcome

SymForge launched from a home, OS, filesystem-root, or otherwise unsafe directory
stays responsive and unbound instead of failing while creating `.symforge`. The
same live process can then index a normal accessible project. A caller may also
explicitly request bounded indexing of an exact protected root; that mode never
touches `<source>/.symforge` and remains queryable with user-local or memory-only
derived state.

## `index_folder` input

```json
{
  "path": "<source-directory>",
  "add": false,
  "allow_protected_root": false,
  "idempotency_key": "<optional-key>"
}
```

`allow_protected_root` defaults to false. It is valid only on this direct tool
request and is included in the canonical idempotency request hash. It is not a
process setting and cannot be inherited by environment discovery, client roots,
reconnect/session metadata, daemon project-open, init, watcher, or snapshot input.
`add=true` may add an explicitly protected project under the same contract; a
failure leaves the existing working set unchanged.

## Source authorization

Both the raw requested path and its canonical target are classified. The stricter
classification wins:

| Class | Meaning |
|---|---|
| `normal` | Ordinary bounded project directory. |
| `protected` | Home/profile, filesystem/drive root, OS/sensitive tree, or broad user/system container. |
| `never_indexable` | Missing/non-directory, device/special namespace, or no stable canonical identity. |

| Request provenance | Normal | Protected | Never indexable |
|---|---|---|---|
| launch CWD, Git ancestor, environment, client root, reconnect/session open | bind normal | remain unbound | remain unbound |
| `index_folder`, flag false/omitted | bind normal | reject request | reject request |
| `index_folder`, flag true | bind normal | bind `explicit_protected` | reject request |
| init | normal mutation checks | refuse | refuse |

An explicit override authorizes only the exact canonical source. It does not grant
structural edit, init, knowledge curation, team-artifact export, `.gitignore`, or
other protected-root mutation authority. A normal but read-only source remains
indexable and exposes mutation capabilities as unavailable.

Rejection never falls through to launch CWD or an undeclared source. A client-
declared candidate list may continue only within that declared list. Failed
canonicalization or retarget leaves an existing project binding, watcher, working
set, and published generation untouched; an unbound process stays unbound.

### Session membership authority

The daemon may share one canonical `ProjectInstance`, but permission to address it is
session-local. Normal validated project routing may create normal membership. Every
session that wants a protected project must issue its own direct
`index_folder(path=<exact>, allow_protected_root=true)` request. A project ID/alias,
`projects=["*"]`, environment/client root, reconnect/session descriptor, snapshot, or
another session's membership cannot grant access. A matching explicit request joins
the existing slot and watcher rather than creating a duplicate. After process
restart, persisted protected state stays dormant until a fresh direct request grants
membership.

The override participates in the normalized request hash. The same idempotency key
with a different override/path conflicts. Replaying a completed `index_folder`
receipt never substitutes for a live binding: the immutable stored execution receipt
is returned only after the current session is attached to the same authorized
project and the live postcondition is re-established. If reattach/rebuild is no
longer possible, the response preserves the stored receipt as history but returns a
successful typed tool result with `applied=false` and
`outcome=live_postcondition_unavailable`; it is not an MCP/protocol error and does
not rewrite the replay record.

## Project state placement

Placement runs only after source authorization:

```text
normal
  -> secure project-local <root>/.symforge succeeds -> project_local
  -> otherwise -> private user-local projects/<project-id>
  -> otherwise -> memory_only

explicit_protected
  -> skip every <root>/.symforge probe
  -> private user-local projects/<project-id>
  -> otherwise -> memory_only
```

The user-local base is the platform's private SymForge application-state location.
The project directory name is the one shared `ProjectId`: a domain-separated,
versioned digest of the lossless canonical native root identity under platform path
equivalence. It never contains the raw path/basename. Canonical aliases share an ID;
distinct worktree roots do not. `ProjectId` chooses placement only; it is not proof
that the repository currently at that path is the one that produced a snapshot.
The same rule applies to durable replay and curation intent: each record binds the
verified repository fingerprint plus `RepositoryId`/`SourceId`, and a foreign record
is quarantined or refused before stored-success replay or source mutation.

`ProjectStateDir` is a private absolute-path newtype constructed only by the
placement resolver. Ownership is closed and explicit:

| Owner | Consumers |
|---|---|
| canonical source root | source/Git reads, relative paths, watcher, repo-owned inputs (including the policy ledger and retained `.symforge/` configuration), and narrowly guarded policy/ignore/team-artifact writes |
| `ProjectStateDir` | snapshot/temp/quarantine/reset/checkpoint, per-project replay/curation intent, coupling/frecency/STEL, analytics, API-key store, edit-safety TEE snapshots, and derived cleanup |
| `ControlStateDir` | edit-safety trust store, sidecar port/PID/session descriptors and status readers, daemon discovery/control, hook adoption/hint state, operator profile, onboarding state, runtime-startup coordination, cross-project `index_folder` replay/locks, and process-global version-registry/update state |
| process memory | live index/watcher/session memberships and labeled non-durable fallbacks |

The project placement resolver runs once per `ProjectInstance`; every state reader,
writer, verifier, and cleanup path receives that same typed result. The control
resolver runs once per process and supplies the same `ControlStatePlacement` to both
descriptor/status readers and writers. Those APIs do not accept a source root and do
not reconstruct `<root>/.symforge` through a compatibility wrapper. The legacy
untyped runtime-data-base oracle is split between those two typed results; analytics
accepts only project state, and TEE receives the bound canonical source and project
state separately. Their CWD-relative `.symforge` fallbacks are removed. An existing
source `.symforge` directory may be read as explicit repo input but never proves
repository identity or writable state placement. Unavailable state disables the
relevant durable feature with a reason.

If either selected `ProjectStateDir` or `ControlStateDir` is inside the source being
indexed (for example explicit indexing of a home tree), each canonical absolute
subtree is a hard dynamic exclusion for scout, watcher, reconciliation, and
verification. This is in addition to the name-based `.symforge/` exclusion and
prevents self-indexing or watcher feedback.

Placement is stable for a `ProjectInstance` lifetime. A later state write failure
changes persistence health/capabilities but not the source, watcher ownership,
published live generation, or query readiness. Reindexing another writable project
selects placement afresh and can restore durable operation without restart.
Re-resolution occurs only when a new `ProjectInstance` is constructed; reindexing the
same live instance never changes its typed placement.

## Process-global control state

Transport/sidecar/daemon discovery and cross-project `index_folder` replay are not
project content state. `ControlStateDir` uses only a safe private user-local base.
It never falls back to launch CWD, a rejected source, a temporary unrelated source,
or relative `.symforge`. If unavailable, coordination/replay is process-local and
explicitly non-durable. Root rejection creates no per-project state entry.
Version-registry and updater state is process-global control state as well; it never
moves when the active project changes. Hook adoption/hints, operator profile,
onboarding, sidecar descriptors, and their status readers use the same resolved
control handle as their writers, so no reader can silently inspect a different CWD
fallback.
Descriptors under the shared control directory are namespaced by `ProjectId` and
daemon/process instance. Writers and status/discovery readers receive that same
namespace key; concurrent projects cannot overwrite or select each other's runtime
descriptor.
Operator profile and onboarding state intentionally become process-global. Legacy
per-project files remain read-only and are not merged; if no global record exists,
onboarding runs once and writes only the shared control owner.

Durable cross-restart idempotency is probed before a first execution's source/state
side effects. In process-local mode, same-process replay/conflict semantics remain
identical; only restart durability is unavailable and reported.

## Watcher and recovery

- Source reads, Git, relative paths, and watcher roots use the canonical source.
- Derived persistence uses only `ProjectStateDir`.
- Global or memory-only placement does not by itself disable watching.
- Watcher failure retains last-valid content behind explicit degraded freshness and
  bounded reconciliation; it is not reported as a persistence failure.
- Snapshot headers bind schema/policy, placement `ProjectId`, stable repository/source
  identity, captured source version, canonical manifest digest, resident-content
  digest, and a repository fingerprint. Working-tree state is the closed
  `Clean`/`Dirty`/`NotApplicable`/`Unknown` value; it is never inferred clean and
  never replaces exact manifest/content verification. A Git fingerprint verifies object format,
  exact HEAD/ref target, tip object, and the reachable history used by temporal
  state; it is never derived only from the Git-directory path. Before Ready or
  overwrite, strong stable-read verification matches that proof, resident hashes,
  and terminal metadata. An unverifiable proof is non-Ready. Same-path replacement,
  including an identical working tree backed by different Git history, cannot
  inherit old content/temporal state. Valid source drift rejects as stale/rebuilds;
  identity collision/corruption is never loaded or overwritten and is quarantined
  when persistent placement exists.
- `checkpoint_now` uses project-local or user-local state. Memory-only mode returns a
  successful, typed “persistence unavailable” operation result without mutating the
  live generation.

## Init and Git hygiene

Automatic startup/scout/watcher/reconciliation/ref ingestion only observe
`.gitignore` hygiene. The shared mutation runs after successful explicit normal
`index_folder` binding and during project-aware init, only for a normal current
repository/worktree with mutation capability:

1. resolve the repository root;
2. if root `.gitignore` is absent, do nothing and do not create it;
3. refuse a symlink/reparse-point file or concurrent hash change;
4. if the repository-root `.gitignore` itself already has an effective ordered rule
   that ignores root `.symforge/`, do nothing; global excludes and
   `.git/info/exclude` do not satisfy repository hygiene;
5. otherwise append canonical `/.symforge/` atomically/idempotently while preserving
   BOM, bytes, line-ending style, and final-newline behavior.

The append never rewrites existing bytes. It uses the first existing newline
sequence (`CRLF` or `LF`), falling back to `LF` when none exists. Empty and BOM-only
files become the preserved optional BOM followed directly by `/.symforge/`, with no
invented final newline. If nonempty content ended in a newline, the appended rule
also ends in that newline; otherwise one separator is inserted before the rule and
the result remains without a final newline. Ordered negation semantics are honored;
an effective equivalent rooted rule in that root file is a no-op.

Protected-root authority never enables init. Scouting always excludes `.symforge/`
regardless of ignore state. A permission or hash-race failure is returned in the
`index_folder`/init receipt and health, but does not roll back the valid source bind
or make live queries unavailable.

The legacy opt-in team artifact remains at project-local
`.symforge/index.bin.zst`. Its export receipt reports one honest Git visibility:
`already_tracked`, `untracked_visible` (for example no root `.gitignore`),
`ignored_force_add_required`, or `git_visibility_unavailable`. Export is refused
unless authorization is normal, source mutation is allowed, and placement is
project-local. It is never redirected to user-local state, and a refusal writes
neither artifact nor `.gitattributes`.

## Health contract

Full and compact health expose these independently:

- binding: `unbound` or `bound`;
- authorization: `normal` or `explicit_protected`;
- state placement: `project_local`, `user_local`, or `memory_only`;
- persistence: healthy/degraded/disabled plus safe fallback reason codes;
- durable replay availability;
- current-session membership authority and live replay postcondition;
- query readiness;
- watcher/freshness state;
- snapshot load/identity state;
- reason-bearing init, curation, edit, checkpoint, and team-artifact capability
  statuses;
- `.gitignore` hygiene when applicable.

`Ready` means the live query generation is usable. It never implies that persistence
is durable or watcher freshness is complete. Unsafe path strings need not be echoed;
health may use the safe project ID and typed reason.

## Contract tests

1. Automatic home/System32/root aliases remain responsive and unbound with no
   source traversal, project watcher, or candidate-root/per-project state I/O.
2. The same process later indexes a normal writable project and reaches Ready.
3. Protected `index_folder` without override rejects; with override it reaches a
   fake/recording queryable index and never touches `<protected>/.symforge`.
4. Raw or canonical protected classification requires override; never-indexable
   targets remain refused with it.
5. Explicit authority is not inherited by init/env/client/reconnect/session open.
6. Normal project-local failure uses user-local state; user-local failure uses a
   queryable memory-only generation.
   A symlink/reparse-point `.symforge` is never followed and forces user-local.
7. Failed retarget preserves the prior source, watcher, and generation.
8. Alias IDs coalesce; same-basename roots and linked worktrees remain isolated.
9. Path-derived placement ID alone cannot validate a snapshot: same-path repository
   replacement and Git-history mismatch remain non-Ready until strong verification,
   and mismatched state is never loaded/overwritten.
10. Nested project and control state are absent from manifest/search/watcher/
    reconciliation.
11. Snapshot/reset/quarantine/checkpoint/replay/coupling/frecency/STEL/analytics/
    API-key/TEE/cleanup consumers receive only their one resolved project-state
    owner; sidecar port/PID/session/status, hooks/adoption/hints, operator profile,
    onboarding, runtime startup, version registry, and updater receive only the one
    resolved control-state owner. Reader and writer paths cannot disagree.
12. Global control failure never creates CWD-relative `.symforge`; replay is labeled
    process-local/non-durable.
13. Memory-only watcher remains live; checkpoint reports unavailable; restart cold
    rebuilds.
14. Explicit normal `index_folder` and project-aware init share a byte-for-byte
    empty/BOM-only/CRLF/LF/final-newline/equivalent/negated/raced/symlinked root
    `.gitignore` matrix; global/info excludes do not satisfy it and automatic paths
    never mutate.
15. Protected/read-only/user-local/memory-only bindings cannot export the team
    artifact or mutate `.gitattributes`; normal export distinguishes tracked,
    untracked-visible, ignored-force-add, and unavailable Git visibility.
16. A second session, reconnect, alias selector, and daemon restart cannot inherit
    protected membership; a fresh exact explicit request joins one existing slot.
17. Override/non-override reuse of one key conflicts, protected `add=true` failure
    preserves the working set, and completed replay re-establishes a live binding or
    returns a successful typed `applied=false`/`live_postcondition_unavailable`
    result rather than stale applied success or an MCP error.
18. Post-bind state-write failure degrades only durability; normal memory-only mode
    exposes reason-bearing mutation capabilities instead of illegal boolean mixes.
19. Health reports binding, membership, placement, durability, readiness, and
    watcher freshness independently.
20. Runtime-data-base, analytics, and TEE discovery have no CWD-relative
    `.symforge` fallback, and a source-owned `.symforge` config path cannot become a
    state-placement or repository-root oracle.
