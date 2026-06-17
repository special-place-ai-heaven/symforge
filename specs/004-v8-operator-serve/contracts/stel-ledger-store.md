# Contract: `StelLedgerStore` (durable economics)

Mirrors `src/analytics/store.rs` `AnalyticsStore` / `SqliteAnalyticsStore`.

## Type
```
enum StelLedgerStore { Sqlite(SqliteStelLedgerStore), Disabled }
```

## Methods
| Method | Behavior |
|--------|----------|
| `open(dir) -> StelLedgerStore` | open/create `stel-ledger.db` in the SymForge data dir; WAL + busy timeout; `migrate()`. On any failure → `Disabled` (logged), never panics (FR-011). |
| `open_in_memory()` | test constructor (mirror analytics) |
| `record(&StelLedgerEvent)` | insert one row into `stel_ledger_events`; no-op when `Disabled` |
| `recent(limit)` | return recent rows (for status/admin) |
| `summary()` | aggregate `net_vs_manual`, accepted-count, session totals; reports unavailable when `Disabled` |
| `schema_version()` | migration guard, mirrors analytics |

## Schema
Table `stel_ledger_events` per [data-model.md](../data-model.md). Indices on `(session_id)`, `(ts_ms)`. `migrate()` is idempotent (assert via test, mirroring `migration_is_idempotent_and_preserves_current_version`).

## Integration
- `src/stel/ledger.rs::capture_ledger` keeps appending to the in-memory `SessionLedger` AND, when the runtime holds a `Sqlite` store, writes the event through.
- Write is off the request hot path (must not add latency to `tools/call`); a failed write degrades to `Disabled`-style logging without failing the call.

## Acceptance (FR-010/011, SC-003)
- Record N events, drop the store, reopen on the same file → N rows present, totals equal.
- Store that cannot open → runtime serves normally; `summary()` reports unavailable; `record()` is a no-op.
