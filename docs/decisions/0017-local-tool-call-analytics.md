# 0017. Local Tool-Call Analytics

Date: 2026-05-19
Status: Accepted

## Context

SymForge currently exposes session-local observability but has no persistent
local tool-call analytics store.

Current source status:

- `src/observability.rs` only initializes tracing.
- `src/lib.rs` exposes `pub mod observability;`.
- `src/sidecar/mod.rs::TokenStats` keeps in-memory per-session tool counts and
  estimated token counters.
- `src/protocol/tools.rs` and `src/protocol/format.rs` render session health,
  hook adoption, token savings, and worktree-misuse counters, but those are not
  a durable analytics history.
- `src/live_index/frecency.rs` and `src/live_index/coupling/store.rs` already
  prove that local SQLite stores can fit SymForge when they are scoped,
  bounded, and explicit.
- `docs/notes/2026-05-16-rtk-match-output-investigation.md` found no real
  `match_output` implementation seam. Analytics must therefore stand or fall on
  its own SymForge observability value, not on a missing RTK output hook.

The product question is whether SymForge should have persistent local tool-call
analytics at all. The answer is yes, but only as a SymForge-native local
observability feature. It is not RTK telemetry, not a command-output filter, and
not a Claude-cost accounting surface.

## Decision

SymForge accepts persistent local tool-call analytics for a future
implementation under the constraints in this ADR.

The feature records bounded metadata about SymForge tool calls so maintainers
and operators can answer questions such as:

- which MCP tools are called most often;
- which tool responses are largest;
- which tools fail most often by coarse failure class;
- which tools consume the most wall-clock time;
- whether an optimization changes local tool behavior over time.

No implementation is added by this ADR. The current production behavior remains
tracing plus session-local counters only.

Initial rollout must be gated, observable, and reversible. This ADR does not
authorize default-on persistent collection. A first implementation must require
explicit enablement, or a later decision must explicitly change that default.

## Data Contract

The analytics store is local-only SQLite at:

```text
dirs::data_local_dir()/symforge/analytics.sqlite3
```

The database must be versioned and migration-safe. Missing state is normal.
Corrupt or unsupported state must report explicit unavailable/degraded evidence
and must not be presented as success.

The minimum `tool_calls` row shape is:

```sql
CREATE TABLE IF NOT EXISTS tool_calls (
    id                 INTEGER PRIMARY KEY,
    recorded_at_utc    TEXT NOT NULL,
    tool_name          TEXT NOT NULL,
    surface            TEXT NOT NULL,
    project_scope_glob TEXT NOT NULL,
    response_bytes     INTEGER NOT NULL,
    estimated_tokens   INTEGER NOT NULL,
    duration_ms        INTEGER NOT NULL,
    success            INTEGER NOT NULL,
    outcome_class      TEXT NOT NULL,
    capability_state   TEXT
);
CREATE INDEX IF NOT EXISTS idx_tool_calls_scope ON tool_calls(project_scope_glob);
CREATE INDEX IF NOT EXISTS idx_tool_calls_time ON tool_calls(recorded_at_utc);
CREATE INDEX IF NOT EXISTS idx_tool_calls_tool ON tool_calls(tool_name);
```

Allowed analytics data is limited to bounded operational metadata:

- public SymForge tool name;
- tool surface such as `mcp` or future `cli`;
- configured project GLOB scope;
- response byte count;
- estimated token count using the existing local heuristic;
- duration;
- success flag;
- bounded outcome class such as `ok`, `not_found`, `invalid_request`,
  `unavailable`, `disabled_by_policy`, or `internal_error`;
- capability state vocabulary from ADR 0016 where relevant.

Forbidden analytics data:

- raw prompts;
- raw query text;
- raw unscoped file paths;
- source snippets or unbounded source blobs;
- `.env` contents;
- provider credentials;
- private keys;
- request payload JSON;
- provider CLI output;
- network telemetry payloads.

Project scoping must use GLOB semantics, not ad-hoc substring matching. If a
call cannot be assigned to an approved scope without storing a raw unscoped path,
the analytics row must use an explicit `unscoped` or `redacted` outcome instead
of persisting the path.

Retention is 90 days. Cleanup must be bounded and local. The implementation may
run cleanup at daemon start or on analytics-store open, but it must not block
ordinary tool responses on long maintenance work.

## Write Path

The hot path must not perform synchronous SQLite inserts.

The accepted shape is:

- a single RAII timer wrapper around the central tool-call dispatch path;
- a bounded `mpsc` queue for analytics events;
- one background writer task or thread owns SQLite writes;
- best-effort WAL mode and a five-second busy timeout, following the existing
  frecency store pattern;
- explicit queue-full, disabled, unavailable, or writer-failed evidence in a
  status surface.

Disabled means no database creation. Discovery-only tool calls must not create
an analytics database when analytics is disabled.

Never hold `RwLock` guards across await points while recording analytics.
Extract owned event data, drop locks, then enqueue.

## Recorded Surfaces

The first implementation is limited to public SymForge MCP tool calls routed
through the existing tool dispatcher.

It may record:

- the canonical public MCP tool name;
- response metadata after the handler completes;
- coarse success or failure class;
- ADR 0016 capability state when the response already computed one.

It must not record:

- background watcher events;
- raw indexing internals that are not user-invoked tools;
- provider CLI sessions;
- shell hooks or command rewriting;
- Cargo or test runner commands;
- analytics reporting commands themselves unless a later ADR says why recursive
  analytics is useful.

## Reporting And Control

The first reporting/control surface is CLI-only on the existing `symforge`
binary. No new MCP analytics tool is authorized by this ADR.

Future CLI surface:

- `symforge analytics status`
- `symforge analytics summary`
- `symforge analytics daily`
- `symforge analytics weekly`
- `symforge analytics monthly`
- `symforge analytics failures`
- `symforge analytics export --format json`
- `symforge analytics export --format csv`
- `symforge analytics reset --yes`

`health` may report one concise analytics status line such as enabled,
disabled, unavailable, queue-full, or writer-failed. It must not dump analytics
rows.

Reset must require explicit confirmation. Export must apply the same redaction
contract as the database schema.

## Future Implementation Goals

A later storage-foundation goal may implement only:

- `src/observability.rs` refactor to `src/observability/mod.rs`;
- `src/observability/analytics.rs`;
- schema versioning, open/migrate, disabled no-footprint behavior, retention,
  and redaction tests.

A later instrumentation/reporting goal may implement only:

- one central dispatcher wrapper, not hand-written inserts in every handler;
- CLI analytics reporting and reset/export commands;
- tests proving disabled mode creates no database, enabled mode writes bounded
  rows, no forbidden raw data is persisted, queue-full behavior is explicit,
  retention deletes old rows, and export/reset obey the CLI contract.

Analytics-trained correction learning is not authorized by this ADR. A separate
decision is required before persistent analytics can train suggestions. Stateless
same-file suggestions remain independent and should proceed without waiting for
analytics.

## Non-Goals

This ADR does not implement analytics.

This ADR does not add `src/observability/analytics.rs`, instrument MCP handlers,
create a database, add CLI commands, add MCP tools, or add migrations.

This ADR does not import RTK runtime code, shell hooks, hook installers, command
rewriting, Claude permission parsing, CLI output filters, OpenClaw plugin code,
Homebrew formula code, HTTP telemetry, or RTK's command/cost schema.

This ADR does not authorize network egress.

This ADR does not authorize default-on collection.

## Consequences

**Easier**

- Future optimization work can be evaluated against local historical behavior
  rather than anecdotes.
- Persistent analytics has a privacy contract before code exists.
- The storage and instrumentation work can be split into focused, testable
  follow-up goals.

**Harder**

- Every analytics event must be deliberately redacted before persistence.
- Tool dispatch must grow a central wrapper without changing public response
  contracts.
- Queue and writer failures need explicit status evidence even though analytics
  is auxiliary.

**New invariants future code must respect**

1. Disabled analytics must not create an analytics database.
2. Analytics rows must never contain raw prompts, raw query text, raw unscoped
   paths, request JSON, source snippets, secrets, or provider credentials.
3. Analytics is local-only; no network telemetry is authorized.
4. Tool responses must not silently treat skipped, unavailable, disabled,
   stale, blocked, queue-full, or writer-failed analytics as success when the
   analytics status is requested.
5. Reporting and reset/export controls start in the CLI, not as new MCP tools.
6. Any later MCP analytics surface requires a separate ADR because it expands
   the public tool surface.
7. Analytics-trained correction learning remains blocked until a separate
   product decision approves it.

## Implementation Status

Accepted as a product and architecture contract only. Production implementation
is pending future goals.
