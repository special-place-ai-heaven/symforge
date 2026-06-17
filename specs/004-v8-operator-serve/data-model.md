# Phase 1 Data Model: Operator Server Spine

## Entities

### ServerRuntime (in-memory, server-only)
The single transport-agnostic owner of request-serving state.

| Field | Type | Notes |
|-------|------|-------|
| `index` | `SharedIndex` | shared with stdio path; one index per server session |
| `protocol` | `McpServer` (existing) | tool dispatch; shared, no fork |
| `governor` | `RequestGovernor` | reused from `sidecar::governor` |
| `auth` | `AuthConfig` | see below |
| `ledger_store` | `StelLedgerStore` | `Sqlite` or `Disabled` |

### AuthConfig (in-memory, server-only)
| Field | Type | Notes |
|-------|------|-------|
| `api_key` | `Option<String>` | single static key (slice scope); `None` = unauthenticated |
| (derived) `requires_auth(bind)` | fn | `true` if `api_key.is_some()` OR bind is non-loopback |

Validation / rules (from FR-002..004):
- non-loopback bind + `api_key.is_none()` → **startup error** (refuse to bind).
- `api_key.is_some()` → every request must present matching Bearer (constant-time).
- loopback bind + `api_key.is_none()` → requests allowed without auth.

### BindAddress
| Field | Type | Notes |
|-------|------|-------|
| `host` | `IpAddr`/string | from `--listen` (default `127.0.0.1`) |
| `port` | `u16` | from `--listen` (default e.g. `8787`); `0` = OS-assigned |
| (derived) `is_loopback` | bool | `127.0.0.0/8` or `::1` |

### SurfaceProfile (existing enum, default changes)
`Compact` (NEW default) | `Full` (opt-out via `SYMFORGE_SURFACE=full`) | `Meta` (`SYMFORGE_SURFACE=meta`). Selected in `protocol::surface_probe::surface_profile_from_env`.

### StelLedgerEvent → table `stel_ledger_events`
Durable form of the in-memory `SessionLedger` rows (`src/stel/ledger.rs`). Columns
persist the real `StelLedgerEvent` fields (`src/stel/types.rs`). The list below is
**as implemented** in `src/stel/ledger_store.rs` (US3/T025-T027), superseding the
Phase-0 draft. Deviations from the draft: the struct carries a `tools_called:
Vec<String>` (not a singular `tool`), persisted as JSON in `tools_called_json`;
`accepted`/`eligible_h6` are not yet fields on the event, so they are reserved
`NULL` columns; and the real struct adds `surface`, `predicted_response_tokens`,
`route_confidence`, `pff_bypass`, `cache_hit`, and `degrade_flags_json`.

| Column | SQLite type | Source field | Notes |
|--------|-------------|--------------|-------|
| `id` | INTEGER PK AUTOINCREMENT | — | row id |
| `ts_ms` | INTEGER NOT NULL | `StelLedgerEvent.ts_ms` | event time (epoch ms) |
| `session_id` | TEXT NOT NULL | store session identity | groups a server session; bounded |
| `plan_id` | TEXT NOT NULL | `StelLedgerEvent.plan_id` | links to L1 plan; bounded |
| `surface` | TEXT NOT NULL | `StelLedgerEvent.surface` | e.g. `symforge`; bounded |
| `intent` | TEXT NOT NULL | `StelLedgerEvent.intent` (`IntentBucket`) | routing bucket; bounded |
| `decision` | TEXT NOT NULL | `StelLedgerEvent.decision` (`AdmissionDecision`) | `serve`/`bypass`/`degrade`/`cache_hit` |
| `tools_called_json` | TEXT NOT NULL | `StelLedgerEvent.tools_called` (`Vec<String>`) | JSON array; bounded |
| `predicted_response_tokens` | INTEGER NOT NULL | `StelLedgerEvent.predicted_response_tokens` | predicted S |
| `actual_response_tokens` | INTEGER NOT NULL | `StelLedgerEvent.actual_response_tokens` | measured S |
| `manual_baseline_tokens` | INTEGER NOT NULL | `StelLedgerEvent.manual_baseline_tokens` | M baseline |
| `net_vs_manual` | INTEGER NOT NULL | `StelLedgerEvent.net_vs_manual` | `M - S`; signed; headline contributor |
| `route_confidence` | TEXT NOT NULL | `StelLedgerEvent.route_confidence` (`RouteConfidence`) | `exact`/`inferred`/`fallback`; bounded |
| `pff_bypass` | INTEGER NULL | `StelLedgerEvent.pff_bypass` (`Option<bool>`) | 0/1/NULL |
| `cache_hit` | INTEGER NULL | `StelLedgerEvent.cache_hit` (`Option<bool>`) | 0/1/NULL |
| `degrade_flags_json` | TEXT NOT NULL | `StelLedgerEvent.degrade_flags` (`Vec<String>`) | JSON array; bounded |
| `accepted` | INTEGER NULL | — (reserved) | not yet on the event; always NULL from `record()` |
| `eligible_h6` | INTEGER NULL | — (reserved) | not yet on the event; always NULL from `record()` |

Schema version lives in a sibling meta table `stel_ledger_meta(key, value)`
(`schema_version` row), not as a column on `stel_ledger_events`. Current schema
version: 1.

Indices: `idx_stel_ledger_events_session (session_id)`, `idx_stel_ledger_events_ts (ts_ms)`. Retention: keep-all today (no cap implemented yet; a retention cap mirroring analytics' `enforce_record_retention` is a later option).

Store states (mirror `AnalyticsStore`): `Sqlite(SqliteStelLedgerStore)` | `Disabled`. `Disabled` is returned when the DB can't open (FR-011) — `record()` is a no-op, `summary()` reports unavailable.

## Relationships
- One `ServerRuntime` → one `StelLedgerStore` → many `stel_ledger_events`.
- `stel_ledger_events.session_id` groups rows per server session; survives restart (new session id per process, history retained).
- `plan_id` optionally links an event to its L1 `StelPlan` (in-memory only; not a FK).
