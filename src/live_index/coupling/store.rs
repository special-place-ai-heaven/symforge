//! Persistent store for co-change coupling evidence.
//!
//! File-backed SQLite via bundled rusqlite. One store per workspace; may
//! share the database file with the frecency store once the wiring for
//! that lands. Step 1.1 scope: open, migrate, upsert, query, and
//! HEAD-oid tracking. Cold-build walker and symbol-identity resolution
//! are separate steps.

use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};

use super::AnchorKey;
use super::decay_factor;
use super::schema::{
    CURRENT_SCHEMA_VERSION, META_DELTA_SINCE_VACUUM, META_LAST_HEAD, META_SCHEMA_VERSION, SCHEMA_V1,
};

/// Run `VACUUM` after this many `commit_delta` applications to reclaim the
/// free pages that incremental updates leave behind. Cold builds VACUUM
/// unconditionally (they `DELETE` every row first), so this only governs the
/// incremental path. Override with `SYMFORGE_COUPLING_VACUUM_EVERY`; `0`
/// disables periodic delta-driven compaction entirely.
pub const DEFAULT_DELTA_VACUUM_INTERVAL: u64 = 50;

/// Env override for the delta-driven VACUUM cadence (`0` disables it).
pub const COUPLING_VACUUM_EVERY_ENV: &str = "SYMFORGE_COUPLING_VACUUM_EVERY";

/// Resolve the delta-driven VACUUM interval from the environment, falling back
/// to [`DEFAULT_DELTA_VACUUM_INTERVAL`]. `0` disables periodic compaction.
/// Unparseable values fall back to the default. Mirrors the testable
/// `*_from_value` pattern used elsewhere in the codebase.
pub fn delta_vacuum_interval_from_value(raw: Option<&str>) -> u64 {
    match raw.map(str::trim) {
        Some(value) if !value.is_empty() => value
            .parse::<u64>()
            .unwrap_or(DEFAULT_DELTA_VACUUM_INTERVAL),
        _ => DEFAULT_DELTA_VACUUM_INTERVAL,
    }
}

fn delta_vacuum_interval_from_env() -> u64 {
    delta_vacuum_interval_from_value(std::env::var(COUPLING_VACUUM_EVERY_ENV).ok().as_deref())
}

#[derive(Debug, Clone, PartialEq)]
pub struct CouplingRow {
    pub anchor: AnchorKey,
    pub partner: AnchorKey,
    pub shared_commits: u32,
    pub weighted_score: f64,
    pub last_commit_ts: i64,
}

/// One entry in the per-commit contribution ledger. `shared_inc` is
/// usually 1 (each commit contributes +1 to its pairs). `base_weight`
/// is `size_weight(anchor_count)` for the commit — the reference-time
/// decay is applied at insert and subtract time so the ledger itself
/// is reference-time-neutral.
#[derive(Debug, Clone, PartialEq)]
pub struct LedgerEdgeRow {
    pub commit_oid: String,
    pub anchor_key: String,
    pub partner_key: String,
    pub shared_inc: u32,
    pub base_weight: f64,
    pub commit_ts: i64,
}

#[derive(Clone)]
pub struct CouplingStore {
    conn: Arc<Mutex<Connection>>,
}

impl CouplingStore {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating coupling db parent dir {:?}", parent))?;
        }
        let conn =
            Connection::open(path).with_context(|| format!("opening coupling db at {:?}", path))?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.migrate()?;
        Ok(store)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory().context("opening in-memory coupling db")?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        let conn = self.conn.lock().expect("coupling mutex poisoned");
        conn.execute_batch(SCHEMA_V1)
            .context("applying coupling schema v1")?;
        conn.execute(
            "INSERT OR IGNORE INTO coupling_meta (key, value) VALUES (?1, ?2)",
            params![META_SCHEMA_VERSION, CURRENT_SCHEMA_VERSION.to_string()],
        )?;
        Ok(())
    }

    pub fn schema_version(&self) -> Result<u32> {
        let conn = self.conn.lock().expect("coupling mutex poisoned");
        let s: Option<String> = conn
            .query_row(
                "SELECT value FROM coupling_meta WHERE key = ?1",
                params![META_SCHEMA_VERSION],
                |r| r.get(0),
            )
            .optional()?;
        Ok(s.and_then(|v| v.parse().ok()).unwrap_or(0))
    }

    pub fn upsert(&self, row: &CouplingRow) -> Result<()> {
        let conn = self.conn.lock().expect("coupling mutex poisoned");
        conn.execute(
            "INSERT INTO coupling
                 (anchor_key, partner_key, shared_commits, weighted_score, last_commit_ts)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(anchor_key, partner_key) DO UPDATE SET
                 shared_commits = excluded.shared_commits,
                 weighted_score = excluded.weighted_score,
                 last_commit_ts = excluded.last_commit_ts",
            params![
                row.anchor.as_str(),
                row.partner.as_str(),
                row.shared_commits,
                row.weighted_score,
                row.last_commit_ts,
            ],
        )?;
        Ok(())
    }

    pub fn bulk_upsert(&self, rows: &[CouplingRow]) -> Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let mut conn = self.conn.lock().expect("coupling mutex poisoned");
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO coupling
                     (anchor_key, partner_key, shared_commits, weighted_score, last_commit_ts)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(anchor_key, partner_key) DO UPDATE SET
                     shared_commits = excluded.shared_commits,
                     weighted_score = excluded.weighted_score,
                     last_commit_ts = excluded.last_commit_ts",
            )?;
            for row in rows {
                stmt.execute(params![
                    row.anchor.as_str(),
                    row.partner.as_str(),
                    row.shared_commits,
                    row.weighted_score,
                    row.last_commit_ts,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Additive merge: for each conflict, sums `shared_commits` and
    /// `weighted_score` with existing values and takes `MAX(last_commit_ts)`.
    /// Use for incremental HEAD-delta updates where new commits contribute
    /// *in addition to* the existing graph (not as a replacement).
    pub fn additive_upsert(&self, rows: &[CouplingRow]) -> Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let mut conn = self.conn.lock().expect("coupling mutex poisoned");
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO coupling
                     (anchor_key, partner_key, shared_commits, weighted_score, last_commit_ts)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(anchor_key, partner_key) DO UPDATE SET
                     shared_commits = coupling.shared_commits + excluded.shared_commits,
                     weighted_score = coupling.weighted_score + excluded.weighted_score,
                     last_commit_ts = MAX(coupling.last_commit_ts, excluded.last_commit_ts)",
            )?;
            for row in rows {
                stmt.execute(params![
                    row.anchor.as_str(),
                    row.partner.as_str(),
                    row.shared_commits,
                    row.weighted_score,
                    row.last_commit_ts,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Atomic rebuild: deletes every row in `coupling` and reinserts `rows`
    /// in one transaction. Use for cold-build semantics where the caller
    /// intends to replace the whole graph. The `coupling_meta` table is
    /// preserved so HEAD-oid and schema-version state survive.
    pub fn replace_all_rows(&self, rows: &[CouplingRow]) -> Result<()> {
        let mut conn = self.conn.lock().expect("coupling mutex poisoned");
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM coupling", [])?;
        if !rows.is_empty() {
            let mut stmt = tx.prepare(
                "INSERT INTO coupling
                     (anchor_key, partner_key, shared_commits, weighted_score, last_commit_ts)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for row in rows {
                stmt.execute(params![
                    row.anchor.as_str(),
                    row.partner.as_str(),
                    row.shared_commits,
                    row.weighted_score,
                    row.last_commit_ts,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn query(&self, anchor: &AnchorKey, limit: u32) -> Result<Vec<CouplingRow>> {
        self.query_with_floor(anchor, limit, 0)
    }

    /// Top-N partners for `anchor`, ordered by `weighted_score DESC`, with a
    /// minimum `shared_commits` floor applied at the SQL layer. ADR 0013 rule 1
    /// — file-level callers pass 2, symbol-level callers pass 3. `floor = 0`
    /// degenerates to `query`.
    pub fn query_with_floor(
        &self,
        anchor: &AnchorKey,
        limit: u32,
        shared_commits_min: u32,
    ) -> Result<Vec<CouplingRow>> {
        let conn = self.conn.lock().expect("coupling mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT anchor_key, partner_key, shared_commits, weighted_score, last_commit_ts
             FROM coupling
             WHERE anchor_key = ?1 AND shared_commits >= ?2
             ORDER BY weighted_score DESC
             LIMIT ?3",
        )?;
        let rows = stmt
            .query_map(params![anchor.as_str(), shared_commits_min, limit], |row| {
                let anchor_s: String = row.get(0)?;
                let partner_s: String = row.get(1)?;
                Ok(CouplingRow {
                    anchor: AnchorKey::from_raw(anchor_s),
                    partner: AnchorKey::from_raw(partner_s),
                    shared_commits: row.get::<_, u32>(2)?,
                    weighted_score: row.get::<_, f64>(3)?,
                    last_commit_ts: row.get::<_, i64>(4)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Exact PK lookup for the ordered pair `(anchor, partner)`. Returns
    /// `None` when the pair is absent. ADR 0013 rule 4 — symbol-gated-by-file
    /// composition looks up the file-level pair corresponding to a symbol-
    /// level pair. Direction matters: the PK at schema.rs:22 is ordered, so
    /// `pair_row(a, b)` and `pair_row(b, a)` are distinct queries.
    pub fn pair_row(&self, anchor: &AnchorKey, partner: &AnchorKey) -> Result<Option<CouplingRow>> {
        let conn = self.conn.lock().expect("coupling mutex poisoned");
        let row = conn
            .query_row(
                "SELECT anchor_key, partner_key, shared_commits, weighted_score, last_commit_ts
                 FROM coupling
                 WHERE anchor_key = ?1 AND partner_key = ?2",
                params![anchor.as_str(), partner.as_str()],
                |row| {
                    let anchor_s: String = row.get(0)?;
                    let partner_s: String = row.get(1)?;
                    Ok(CouplingRow {
                        anchor: AnchorKey::from_raw(anchor_s),
                        partner: AnchorKey::from_raw(partner_s),
                        shared_commits: row.get::<_, u32>(2)?,
                        weighted_score: row.get::<_, f64>(3)?,
                        last_commit_ts: row.get::<_, i64>(4)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    pub fn last_head(&self) -> Result<Option<String>> {
        let conn = self.conn.lock().expect("coupling mutex poisoned");
        let s: Option<String> = conn
            .query_row(
                "SELECT value FROM coupling_meta WHERE key = ?1",
                params![META_LAST_HEAD],
                |r| r.get(0),
            )
            .optional()?;
        Ok(s)
    }

    pub fn set_last_head(&self, head_oid: &str) -> Result<()> {
        let conn = self.conn.lock().expect("coupling mutex poisoned");
        conn.execute(
            "INSERT INTO coupling_meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![META_LAST_HEAD, head_oid],
        )?;
        Ok(())
    }

    pub fn cold_built_at(&self) -> Result<Option<i64>> {
        let conn = self.conn.lock().expect("coupling mutex poisoned");
        let s: Option<String> = conn
            .query_row(
                "SELECT value FROM coupling_meta WHERE key = ?1",
                params![super::schema::META_COLD_BUILT_AT],
                |r| r.get(0),
            )
            .optional()?;
        Ok(s.and_then(|v| v.parse().ok()))
    }

    pub fn set_cold_built_at(&self, ts: i64) -> Result<()> {
        let conn = self.conn.lock().expect("coupling mutex poisoned");
        conn.execute(
            "INSERT INTO coupling_meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![super::schema::META_COLD_BUILT_AT, ts.to_string()],
        )?;
        Ok(())
    }

    pub fn last_reference_ts(&self) -> Result<Option<i64>> {
        let conn = self.conn.lock().expect("coupling mutex poisoned");
        let s: Option<String> = conn
            .query_row(
                "SELECT value FROM coupling_meta WHERE key = ?1",
                params![super::schema::META_LAST_REFERENCE_TS],
                |r| r.get(0),
            )
            .optional()?;
        Ok(s.and_then(|v| v.parse().ok()))
    }

    pub fn set_last_reference_ts(&self, ts: i64) -> Result<()> {
        let conn = self.conn.lock().expect("coupling mutex poisoned");
        conn.execute(
            "INSERT INTO coupling_meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![super::schema::META_LAST_REFERENCE_TS, ts.to_string()],
        )?;
        Ok(())
    }

    /// Reclaim free pages by rewriting the database file. SQLite never returns
    /// pages freed by `DELETE` to the OS on its own (auto_vacuum is off), so
    /// the file grows monotonically with churn until a `VACUUM` repacks it.
    /// Must run outside any transaction.
    pub fn vacuum(&self) -> Result<()> {
        let conn = self.conn.lock().expect("coupling mutex poisoned");
        conn.execute_batch("VACUUM")
            .context("vacuuming coupling db")?;
        Ok(())
    }

    /// Number of `commit_delta` applications recorded since the last VACUUM.
    /// Reset to 0 by cold builds (which VACUUM unconditionally).
    pub fn delta_since_vacuum(&self) -> Result<u64> {
        let conn = self.conn.lock().expect("coupling mutex poisoned");
        let s: Option<String> = conn
            .query_row(
                "SELECT value FROM coupling_meta WHERE key = ?1",
                params![META_DELTA_SINCE_VACUUM],
                |r| r.get(0),
            )
            .optional()?;
        Ok(s.and_then(|v| v.parse().ok()).unwrap_or(0))
    }

    /// Increment the persisted delta-since-vacuum counter and, when the
    /// configured interval is reached, VACUUM and reset the counter. The
    /// counter is bumped inside the delta transaction (see `commit_delta`);
    /// this method is the post-commit "maybe compact" step, so it reads the
    /// freshly-committed counter and acts on it. A `0` interval disables
    /// delta-driven compaction. VACUUM failures are non-fatal.
    fn maybe_vacuum_after_delta(&self) {
        let interval = delta_vacuum_interval_from_env();
        if interval == 0 {
            return;
        }
        let count = match self.delta_since_vacuum() {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("coupling: reading delta-vacuum counter failed: {e}");
                return;
            }
        };
        if count < interval {
            return;
        }
        if let Err(e) = self.vacuum() {
            tracing::warn!("coupling: periodic delta VACUUM failed: {e}");
            return;
        }
        if let Err(e) = self.reset_delta_since_vacuum() {
            tracing::warn!("coupling: resetting delta-vacuum counter failed: {e}");
        }
    }

    /// Reset the persisted delta-since-vacuum counter to 0.
    fn reset_delta_since_vacuum(&self) -> Result<()> {
        let conn = self.conn.lock().expect("coupling mutex poisoned");
        conn.execute(
            "INSERT INTO coupling_meta (key, value) VALUES (?1, '0')
             ON CONFLICT(key) DO UPDATE SET value = '0'",
            params![META_DELTA_SINCE_VACUUM],
        )?;
        Ok(())
    }

    /// Snapshot of the commit OIDs currently inside the bounded window,
    /// keyed to their commit timestamp. Used by delta to diff against a
    /// freshly-computed window.
    pub fn active_commit_oids(&self) -> Result<std::collections::HashMap<String, i64>> {
        let conn = self.conn.lock().expect("coupling mutex poisoned");
        let mut stmt = conn.prepare("SELECT commit_oid, commit_ts FROM coupling_active_commits")?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows.into_iter().collect())
    }

    /// Atomic cold build: purges `coupling`, `coupling_active_commits`, and
    /// `coupling_commit_edges`, writes the supplied state, and updates meta
    /// (last_head, last_reference_ts, cold_built_at) in a single transaction.
    pub fn commit_cold_build(
        &self,
        new_rows: &[CouplingRow],
        new_commits: &[(String, i64)],
        new_ledger: &[LedgerEdgeRow],
        new_head: Option<&str>,
        reference_ts: i64,
    ) -> Result<()> {
        let mut conn = self.conn.lock().expect("coupling mutex poisoned");
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM coupling", [])?;
        tx.execute("DELETE FROM coupling_active_commits", [])?;
        tx.execute("DELETE FROM coupling_commit_edges", [])?;

        if !new_rows.is_empty() {
            let mut stmt = tx.prepare(
                "INSERT INTO coupling
                     (anchor_key, partner_key, shared_commits, weighted_score, last_commit_ts)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for row in new_rows {
                stmt.execute(params![
                    row.anchor.as_str(),
                    row.partner.as_str(),
                    row.shared_commits,
                    row.weighted_score,
                    row.last_commit_ts,
                ])?;
            }
        }
        if !new_commits.is_empty() {
            let mut stmt = tx.prepare(
                "INSERT INTO coupling_active_commits (commit_oid, commit_ts) VALUES (?1, ?2)",
            )?;
            for (oid, ts) in new_commits {
                stmt.execute(params![oid, ts])?;
            }
        }
        if !new_ledger.is_empty() {
            let mut stmt = tx.prepare(
                "INSERT INTO coupling_commit_edges
                     (commit_oid, anchor_key, partner_key, shared_inc, base_weight, commit_ts)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for edge in new_ledger {
                stmt.execute(params![
                    edge.commit_oid,
                    edge.anchor_key,
                    edge.partner_key,
                    edge.shared_inc,
                    edge.base_weight,
                    edge.commit_ts,
                ])?;
            }
        }
        match new_head {
            Some(head) => {
                tx.execute(
                    "INSERT INTO coupling_meta (key, value) VALUES (?1, ?2)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    params![super::schema::META_LAST_HEAD, head],
                )?;
            }
            None => {
                // HEAD was lost — forget the recorded head so a later delta
                // cannot NoOp-fast-path back into an empty store.
                tx.execute(
                    "DELETE FROM coupling_meta WHERE key = ?1",
                    params![super::schema::META_LAST_HEAD],
                )?;
            }
        }
        tx.execute(
            "INSERT INTO coupling_meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![
                super::schema::META_LAST_REFERENCE_TS,
                reference_ts.to_string()
            ],
        )?;
        tx.execute(
            "INSERT INTO coupling_meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![super::schema::META_COLD_BUILT_AT, reference_ts.to_string()],
        )?;
        // A cold build always VACUUMs (below), so the delta-since-vacuum
        // counter starts fresh from here.
        tx.execute(
            "INSERT INTO coupling_meta (key, value) VALUES (?1, '0')
             ON CONFLICT(key) DO UPDATE SET value = '0'",
            params![META_DELTA_SINCE_VACUUM],
        )?;
        tx.commit()?;
        drop(conn);

        // Cold build `DELETE`s every row before reinserting, orphaning a large
        // number of pages. VACUUM unconditionally to return them to the OS;
        // cold builds are rare (first session / wipe / schema reset), so the
        // cost is acceptable and this is the single highest-leverage bound on
        // coupling.db growth. Non-fatal on failure.
        if let Err(e) = self.vacuum() {
            tracing::warn!("coupling: post-cold-build VACUUM failed: {e}");
        }
        Ok(())
    }

    /// Atomic incremental delta. Rescales all existing `coupling` rows from
    /// `old_reference_ts` to `new_reference_ts`, subtracts outgoing commits
    /// (using their ledger entries with contributions computed at
    /// `new_reference_ts`), inserts incoming ledger entries and adds their
    /// contributions, recomputes `last_commit_ts` and prunes empty pairs.
    /// Updates `last_head` and `last_reference_ts`.
    ///
    /// `half_life_secs` must match the half-life the store was built under
    /// (callers are responsible for consistency).
    #[allow(clippy::too_many_arguments)]
    pub fn commit_delta(
        &self,
        incoming_commits: &[(String, i64)],
        incoming_ledger: &[LedgerEdgeRow],
        outgoing_oids: &[String],
        new_head: Option<&str>,
        old_reference_ts: Option<i64>,
        new_reference_ts: i64,
        half_life_secs: i64,
    ) -> Result<()> {
        let mut conn = self.conn.lock().expect("coupling mutex poisoned");
        let tx = conn.transaction()?;

        // Step 1: rescale existing aggregate rows from old_ref to new_ref.
        if let Some(old_ref) = old_reference_ts
            && old_ref != new_reference_ts
        {
            let factor = decay_factor(new_reference_ts - old_ref, half_life_secs);
            if (factor - 1.0).abs() > f64::EPSILON {
                tx.execute(
                    "UPDATE coupling SET weighted_score = weighted_score * ?1",
                    params![factor],
                )?;
            }
        }

        // Touched pairs — recompute last_commit_ts for these at the end.
        let mut touched_pairs: std::collections::HashSet<(String, String)> =
            std::collections::HashSet::new();

        // Step 2: subtract outgoing contributions and delete ledger rows.
        if !outgoing_oids.is_empty() {
            // Load ledger rows for outgoing commits, subtract contributions
            // computed at new_reference_ts.
            let mut select = tx.prepare(
                "SELECT anchor_key, partner_key, shared_inc, base_weight, commit_ts
                 FROM coupling_commit_edges
                 WHERE commit_oid = ?1",
            )?;
            let mut update = tx.prepare(
                "UPDATE coupling
                    SET shared_commits = shared_commits - ?1,
                        weighted_score = weighted_score - ?2
                  WHERE anchor_key = ?3 AND partner_key = ?4",
            )?;
            for oid in outgoing_oids {
                let rows = select
                    .query_map(params![oid], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, u32>(2)?,
                            row.get::<_, f64>(3)?,
                            row.get::<_, i64>(4)?,
                        ))
                    })?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                for (anchor, partner, shared_inc, base_weight, commit_ts) in rows {
                    let contribution =
                        base_weight * decay_factor(new_reference_ts - commit_ts, half_life_secs);
                    update.execute(params![shared_inc, contribution, anchor, partner])?;
                    touched_pairs.insert((anchor, partner));
                }
            }
            drop(select);
            drop(update);

            // Delete outgoing ledger rows and active_commits entries.
            let mut del_edges =
                tx.prepare("DELETE FROM coupling_commit_edges WHERE commit_oid = ?1")?;
            let mut del_active =
                tx.prepare("DELETE FROM coupling_active_commits WHERE commit_oid = ?1")?;
            for oid in outgoing_oids {
                del_edges.execute(params![oid])?;
                del_active.execute(params![oid])?;
            }
        }

        // Step 3: insert incoming active_commits and ledger rows, apply
        // their contributions additively.
        if !incoming_commits.is_empty() {
            let mut stmt = tx.prepare(
                "INSERT INTO coupling_active_commits (commit_oid, commit_ts) VALUES (?1, ?2)",
            )?;
            for (oid, ts) in incoming_commits {
                stmt.execute(params![oid, ts])?;
            }
        }
        if !incoming_ledger.is_empty() {
            let mut ledger_insert = tx.prepare(
                "INSERT INTO coupling_commit_edges
                     (commit_oid, anchor_key, partner_key, shared_inc, base_weight, commit_ts)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            let mut agg_upsert = tx.prepare(
                "INSERT INTO coupling
                     (anchor_key, partner_key, shared_commits, weighted_score, last_commit_ts)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(anchor_key, partner_key) DO UPDATE SET
                     shared_commits = coupling.shared_commits + excluded.shared_commits,
                     weighted_score = coupling.weighted_score + excluded.weighted_score,
                     last_commit_ts = MAX(coupling.last_commit_ts, excluded.last_commit_ts)",
            )?;
            for edge in incoming_ledger {
                ledger_insert.execute(params![
                    edge.commit_oid,
                    edge.anchor_key,
                    edge.partner_key,
                    edge.shared_inc,
                    edge.base_weight,
                    edge.commit_ts,
                ])?;
                let contribution = edge.base_weight
                    * decay_factor(new_reference_ts - edge.commit_ts, half_life_secs);
                agg_upsert.execute(params![
                    edge.anchor_key,
                    edge.partner_key,
                    edge.shared_inc,
                    contribution,
                    edge.commit_ts,
                ])?;
                touched_pairs.insert((edge.anchor_key.clone(), edge.partner_key.clone()));
            }
        }

        // Step 4: prune pairs whose shared_commits dropped to zero, then
        // recompute last_commit_ts from remaining ledger rows for pairs
        // whose ledger changed.
        tx.execute("DELETE FROM coupling WHERE shared_commits <= 0", [])?;

        if !touched_pairs.is_empty() {
            let mut update_last = tx.prepare(
                "UPDATE coupling
                    SET last_commit_ts = (
                        SELECT MAX(commit_ts) FROM coupling_commit_edges
                         WHERE anchor_key = ?1 AND partner_key = ?2
                    )
                  WHERE anchor_key = ?1 AND partner_key = ?2",
            )?;
            for (anchor, partner) in &touched_pairs {
                update_last.execute(params![anchor, partner])?;
            }
        }

        // Step 5: update meta.
        match new_head {
            Some(head) => {
                tx.execute(
                    "INSERT INTO coupling_meta (key, value) VALUES (?1, ?2)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    params![super::schema::META_LAST_HEAD, head],
                )?;
            }
            None => {
                // HEAD was lost — forget the recorded head so a later delta
                // cannot NoOp-fast-path back into an empty store.
                tx.execute(
                    "DELETE FROM coupling_meta WHERE key = ?1",
                    params![super::schema::META_LAST_HEAD],
                )?;
            }
        }
        tx.execute(
            "INSERT INTO coupling_meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![
                super::schema::META_LAST_REFERENCE_TS,
                new_reference_ts.to_string()
            ],
        )?;

        // Bump the delta-since-vacuum counter inside the same transaction so it
        // advances exactly once per committed delta and survives restarts.
        // Stored as TEXT to match the other meta values; COALESCE handles the
        // first delta after a cold build / fresh store.
        tx.execute(
            "INSERT INTO coupling_meta (key, value) VALUES (?1, '1')
             ON CONFLICT(key) DO UPDATE SET
                 value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)",
            params![META_DELTA_SINCE_VACUUM],
        )?;

        tx.commit()?;
        drop(conn);

        // After committing, compact if the configured interval is reached.
        // Deltas churn pages via the global rescale UPDATE; periodic VACUUM
        // bounds the slow growth the incremental path leaves behind.
        self.maybe_vacuum_after_delta();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_row(anchor: &str, partner: &str, shared: u32, weighted: f64, ts: i64) -> CouplingRow {
        CouplingRow {
            anchor: AnchorKey::file(anchor),
            partner: AnchorKey::file(partner),
            shared_commits: shared,
            weighted_score: weighted,
            last_commit_ts: ts,
        }
    }

    #[test]
    fn open_in_memory_creates_fresh_schema_at_current_version() {
        let store = CouplingStore::open_in_memory().unwrap();
        assert_eq!(store.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    }

    #[test]
    fn migrate_is_idempotent() {
        let store = CouplingStore::open_in_memory().unwrap();
        store.migrate().unwrap();
        store.migrate().unwrap();
        assert_eq!(store.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    }

    #[test]
    fn upsert_then_query_returns_the_row() {
        let store = CouplingStore::open_in_memory().unwrap();
        let row = sample_row("src/a.rs", "src/b.rs", 3, 12.5, 1_700_000_000);
        store.upsert(&row).unwrap();
        let got = store.query(&row.anchor, 10).unwrap();
        assert_eq!(got, vec![row]);
    }

    #[test]
    fn upsert_twice_overwrites_latest_values() {
        let store = CouplingStore::open_in_memory().unwrap();
        let a = AnchorKey::file("src/a.rs");
        store
            .upsert(&sample_row("src/a.rs", "src/b.rs", 1, 1.0, 100))
            .unwrap();
        store
            .upsert(&sample_row("src/a.rs", "src/b.rs", 7, 9.9, 200))
            .unwrap();
        let got = store.query(&a, 10).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].shared_commits, 7);
        assert_eq!(got[0].weighted_score, 9.9);
        assert_eq!(got[0].last_commit_ts, 200);
    }

    #[test]
    fn query_orders_partners_by_weighted_score_desc() {
        let store = CouplingStore::open_in_memory().unwrap();
        let a = AnchorKey::file("src/a.rs");
        store
            .bulk_upsert(&[
                sample_row("src/a.rs", "src/low.rs", 1, 1.0, 10),
                sample_row("src/a.rs", "src/high.rs", 5, 50.0, 20),
                sample_row("src/a.rs", "src/mid.rs", 2, 20.0, 15),
            ])
            .unwrap();
        let got = store.query(&a, 10).unwrap();
        let keys: Vec<&str> = got.iter().map(|r| r.partner.as_str()).collect();
        assert_eq!(
            keys,
            vec!["file:src/high.rs", "file:src/mid.rs", "file:src/low.rs"]
        );
    }

    #[test]
    fn query_respects_limit() {
        let store = CouplingStore::open_in_memory().unwrap();
        let a = AnchorKey::file("src/a.rs");
        store
            .bulk_upsert(&[
                sample_row("src/a.rs", "src/b.rs", 1, 3.0, 10),
                sample_row("src/a.rs", "src/c.rs", 1, 2.0, 10),
                sample_row("src/a.rs", "src/d.rs", 1, 1.0, 10),
            ])
            .unwrap();
        let got = store.query(&a, 2).unwrap();
        assert_eq!(got.len(), 2);
    }

    #[test]
    fn query_returns_empty_for_unknown_anchor() {
        let store = CouplingStore::open_in_memory().unwrap();
        let got = store
            .query(&AnchorKey::file("does/not/exist.rs"), 10)
            .unwrap();
        assert!(got.is_empty());
    }

    #[test]
    fn bulk_upsert_empty_slice_is_noop() {
        let store = CouplingStore::open_in_memory().unwrap();
        store.bulk_upsert(&[]).unwrap();
    }

    #[test]
    fn last_head_is_none_initially() {
        let store = CouplingStore::open_in_memory().unwrap();
        assert_eq!(store.last_head().unwrap(), None);
    }

    #[test]
    fn set_last_head_persists_and_overwrites() {
        let store = CouplingStore::open_in_memory().unwrap();
        store.set_last_head("oid-first").unwrap();
        assert_eq!(store.last_head().unwrap().as_deref(), Some("oid-first"));
        store.set_last_head("oid-second").unwrap();
        assert_eq!(store.last_head().unwrap().as_deref(), Some("oid-second"));
    }

    #[test]
    fn replace_all_rows_purges_existing_and_inserts_new() {
        let store = CouplingStore::open_in_memory().unwrap();
        store
            .bulk_upsert(&[
                sample_row("stale/a.rs", "stale/b.rs", 5, 10.0, 100),
                sample_row("stale/c.rs", "stale/d.rs", 3, 7.0, 50),
            ])
            .unwrap();

        store
            .replace_all_rows(&[sample_row("fresh/x.rs", "fresh/y.rs", 1, 2.0, 999)])
            .unwrap();

        // Stale rows must be gone.
        assert!(
            store
                .query(&AnchorKey::file("stale/a.rs"), 10)
                .unwrap()
                .is_empty()
        );
        assert!(
            store
                .query(&AnchorKey::file("stale/c.rs"), 10)
                .unwrap()
                .is_empty()
        );

        // Fresh row present.
        let fresh = store.query(&AnchorKey::file("fresh/x.rs"), 10).unwrap();
        assert_eq!(fresh.len(), 1);
        assert_eq!(fresh[0].partner, AnchorKey::file("fresh/y.rs"));
    }

    #[test]
    fn additive_upsert_sums_values_and_takes_max_ts() {
        let store = CouplingStore::open_in_memory().unwrap();
        store
            .upsert(&sample_row("a.rs", "b.rs", 3, 7.5, 100))
            .unwrap();
        store
            .additive_upsert(&[sample_row("a.rs", "b.rs", 2, 4.0, 50)])
            .unwrap();
        let got = store.query(&AnchorKey::file("a.rs"), 1).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].shared_commits, 5);
        assert!((got[0].weighted_score - 11.5).abs() < 1e-9);
        assert_eq!(got[0].last_commit_ts, 100, "max of 100 and 50");

        store
            .additive_upsert(&[sample_row("a.rs", "b.rs", 1, 1.0, 500)])
            .unwrap();
        let got2 = store.query(&AnchorKey::file("a.rs"), 1).unwrap();
        assert_eq!(got2[0].shared_commits, 6);
        assert!((got2[0].weighted_score - 12.5).abs() < 1e-9);
        assert_eq!(got2[0].last_commit_ts, 500, "new max");
    }

    #[test]
    fn additive_upsert_inserts_new_pair_when_no_conflict() {
        let store = CouplingStore::open_in_memory().unwrap();
        store
            .additive_upsert(&[sample_row("x.rs", "y.rs", 2, 3.0, 42)])
            .unwrap();
        let got = store.query(&AnchorKey::file("x.rs"), 10).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].shared_commits, 2);
    }

    #[test]
    fn additive_upsert_empty_slice_is_noop() {
        let store = CouplingStore::open_in_memory().unwrap();
        store
            .upsert(&sample_row("a.rs", "b.rs", 1, 1.0, 0))
            .unwrap();
        store.additive_upsert(&[]).unwrap();
        let got = store.query(&AnchorKey::file("a.rs"), 10).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].shared_commits, 1);
    }

    #[test]
    fn replace_all_rows_with_empty_slice_clears_table() {
        let store = CouplingStore::open_in_memory().unwrap();
        store
            .bulk_upsert(&[sample_row("a.rs", "b.rs", 1, 1.0, 0)])
            .unwrap();
        store.replace_all_rows(&[]).unwrap();
        assert!(
            store
                .query(&AnchorKey::file("a.rs"), 10)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn replace_all_rows_preserves_coupling_meta() {
        let store = CouplingStore::open_in_memory().unwrap();
        store.set_last_head("head-oid-abc").unwrap();
        store
            .bulk_upsert(&[sample_row("a.rs", "b.rs", 1, 1.0, 0)])
            .unwrap();
        store.replace_all_rows(&[]).unwrap();
        // Meta survives.
        assert_eq!(store.last_head().unwrap().as_deref(), Some("head-oid-abc"));
        assert_eq!(store.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    }

    #[test]
    fn cold_built_at_is_none_before_writes_persist_after() {
        let store = CouplingStore::open_in_memory().unwrap();
        assert_eq!(store.cold_built_at().unwrap(), None);
        store.set_cold_built_at(1_700_000_000).unwrap();
        assert_eq!(store.cold_built_at().unwrap(), Some(1_700_000_000));
    }

    #[test]
    fn open_file_backed_creates_db_and_parent_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("nested").join("coupling.db");
        let store = CouplingStore::open(&db_path).unwrap();
        assert_eq!(store.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
        assert!(db_path.exists());
    }

    #[test]
    fn open_existing_file_reuses_schema() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("coupling.db");
        {
            let store = CouplingStore::open(&db_path).unwrap();
            store
                .upsert(&sample_row("src/a.rs", "src/b.rs", 1, 5.0, 10))
                .unwrap();
        }
        let reopened = CouplingStore::open(&db_path).unwrap();
        let got = reopened.query(&AnchorKey::file("src/a.rs"), 10).unwrap();
        assert_eq!(got.len(), 1);
    }

    #[test]
    fn query_with_floor_excludes_weak_pairs() {
        let store = CouplingStore::open_in_memory().unwrap();
        let a = AnchorKey::file("src/a.rs");
        store
            .bulk_upsert(&[
                sample_row("src/a.rs", "src/p1.rs", 1, 10.0, 10),
                sample_row("src/a.rs", "src/p2.rs", 2, 20.0, 20),
                sample_row("src/a.rs", "src/p3.rs", 3, 30.0, 30),
            ])
            .unwrap();
        let got = store.query_with_floor(&a, 10, 2).unwrap();
        let partners: Vec<&str> = got.iter().map(|r| r.partner.as_str()).collect();
        assert_eq!(
            partners,
            vec!["file:src/p3.rs", "file:src/p2.rs"],
            "floor=2 must exclude the shared=1 pair"
        );
    }

    #[test]
    fn query_with_floor_keeps_strong_pairs() {
        let store = CouplingStore::open_in_memory().unwrap();
        let a = AnchorKey::file("src/a.rs");
        store
            .bulk_upsert(&[
                sample_row("src/a.rs", "src/p1.rs", 3, 10.0, 10),
                sample_row("src/a.rs", "src/p2.rs", 5, 20.0, 20),
                sample_row("src/a.rs", "src/p3.rs", 10, 30.0, 30),
            ])
            .unwrap();
        let got = store.query_with_floor(&a, 10, 3).unwrap();
        assert_eq!(got.len(), 3, "floor=3 keeps all pairs with shared >= 3");
    }

    #[test]
    fn query_with_floor_zero_matches_query_behavior() {
        let store = CouplingStore::open_in_memory().unwrap();
        let a = AnchorKey::file("src/a.rs");
        store
            .bulk_upsert(&[
                sample_row("src/a.rs", "src/p1.rs", 1, 3.0, 10),
                sample_row("src/a.rs", "src/p2.rs", 4, 15.0, 20),
                sample_row("src/a.rs", "src/p3.rs", 2, 8.0, 30),
            ])
            .unwrap();
        let plain = store.query(&a, 10).unwrap();
        let with_zero = store.query_with_floor(&a, 10, 0).unwrap();
        assert_eq!(plain, with_zero, "floor=0 is a no-op filter");
    }

    #[test]
    fn query_with_floor_preserves_weighted_score_ordering() {
        let store = CouplingStore::open_in_memory().unwrap();
        let a = AnchorKey::file("src/a.rs");
        store
            .bulk_upsert(&[
                sample_row("src/a.rs", "src/p1.rs", 1, 5.0, 10),
                sample_row("src/a.rs", "src/p2.rs", 2, 100.0, 20),
                sample_row("src/a.rs", "src/p3.rs", 3, 50.0, 30),
                sample_row("src/a.rs", "src/p4.rs", 4, 25.0, 40),
            ])
            .unwrap();
        let got = store.query_with_floor(&a, 10, 2).unwrap();
        let partners: Vec<&str> = got.iter().map(|r| r.partner.as_str()).collect();
        assert_eq!(
            partners,
            vec!["file:src/p2.rs", "file:src/p3.rs", "file:src/p4.rs"],
            "floor=2 drops shared=1 pair, remainder stays in weighted_score DESC"
        );
    }

    #[test]
    fn query_with_floor_above_max_returns_empty() {
        let store = CouplingStore::open_in_memory().unwrap();
        let a = AnchorKey::file("src/a.rs");
        store
            .bulk_upsert(&[
                sample_row("src/a.rs", "src/b.rs", 3, 10.0, 10),
                sample_row("src/a.rs", "src/c.rs", 5, 20.0, 20),
            ])
            .unwrap();
        let got = store.query_with_floor(&a, 10, 100).unwrap();
        assert!(got.is_empty(), "floor above any stored value returns empty");
    }

    #[test]
    fn pair_row_returns_row_when_pair_exists() {
        let store = CouplingStore::open_in_memory().unwrap();
        let a = AnchorKey::file("src/a.rs");
        let b = AnchorKey::file("src/b.rs");
        store
            .upsert(&sample_row("src/a.rs", "src/b.rs", 5, 2.5, 1_700_000_000))
            .unwrap();
        let got = store.pair_row(&a, &b).unwrap();
        let row = got.expect("pair present");
        assert_eq!(row.anchor, a);
        assert_eq!(row.partner, b);
        assert_eq!(row.shared_commits, 5);
        assert!((row.weighted_score - 2.5).abs() < 1e-9);
        assert_eq!(row.last_commit_ts, 1_700_000_000);
    }

    #[test]
    fn pair_row_returns_none_for_absent_pair() {
        let store = CouplingStore::open_in_memory().unwrap();
        store
            .upsert(&sample_row("src/a.rs", "src/b.rs", 1, 1.0, 0))
            .unwrap();
        let got = store
            .pair_row(&AnchorKey::file("src/a.rs"), &AnchorKey::file("src/c.rs"))
            .unwrap();
        assert!(got.is_none());
    }

    #[test]
    fn pair_row_returns_none_for_unknown_anchor() {
        let store = CouplingStore::open_in_memory().unwrap();
        let got = store
            .pair_row(&AnchorKey::file("x.rs"), &AnchorKey::file("y.rs"))
            .unwrap();
        assert!(got.is_none(), "empty store returns None for any pair");
    }

    #[test]
    fn pair_row_is_directional() {
        let store = CouplingStore::open_in_memory().unwrap();
        let a = AnchorKey::file("src/a.rs");
        let b = AnchorKey::file("src/b.rs");
        // Insert only one direction via `upsert` — bulk_upsert is symmetric
        // in production, but pair_row's PK contract must work even when a
        // caller constructs a single-direction pair.
        store
            .upsert(&sample_row("src/a.rs", "src/b.rs", 3, 7.5, 100))
            .unwrap();
        assert!(store.pair_row(&a, &b).unwrap().is_some(), "(a, b) exists");
        assert!(
            store.pair_row(&b, &a).unwrap().is_none(),
            "reverse direction absent — PK is ordered"
        );
    }

    #[test]
    fn pair_row_does_not_filter_by_shared_commits() {
        let store = CouplingStore::open_in_memory().unwrap();
        let a = AnchorKey::file("src/a.rs");
        let b = AnchorKey::file("src/b.rs");
        store
            .upsert(&sample_row("src/a.rs", "src/b.rs", 1, 0.5, 50))
            .unwrap();
        let got = store.pair_row(&a, &b).unwrap();
        let row = got.expect("weak pair still retrievable");
        assert_eq!(
            row.shared_commits, 1,
            "pair_row returns row regardless of strength; floor is caller-applied"
        );
    }

    // ─── Compaction / VACUUM bounding ───────────────────────────────────

    use std::sync::Mutex as StdMutex;

    // Serialises COUPLING_VACUUM_EVERY_ENV mutation across tests. Project test
    // policy already enforces --test-threads=1, but the lock makes the env
    // contract explicit and robust to future parallelism.
    static VACUUM_ENV_LOCK: StdMutex<()> = StdMutex::new(());

    #[allow(unsafe_code)] // test-only env helper runs under VACUUM_ENV_LOCK.
    fn set_vacuum_env(value: &str) {
        // SAFETY: callers hold VACUUM_ENV_LOCK; tests run single-threaded.
        unsafe { std::env::set_var(COUPLING_VACUUM_EVERY_ENV, value) };
    }

    #[allow(unsafe_code)] // test-only env helper runs under VACUUM_ENV_LOCK.
    fn clear_vacuum_env() {
        // SAFETY: callers hold VACUUM_ENV_LOCK; tests run single-threaded.
        unsafe { std::env::remove_var(COUPLING_VACUUM_EVERY_ENV) };
    }

    fn ledger(oid: &str, anchor: &str, partner: &str, ts: i64) -> LedgerEdgeRow {
        LedgerEdgeRow {
            commit_oid: oid.to_string(),
            anchor_key: AnchorKey::file(anchor).as_str().to_string(),
            partner_key: AnchorKey::file(partner).as_str().to_string(),
            shared_inc: 1,
            base_weight: 1.0,
            commit_ts: ts,
        }
    }

    #[test]
    fn delta_vacuum_interval_parses_and_defaults() {
        assert_eq!(
            delta_vacuum_interval_from_value(None),
            DEFAULT_DELTA_VACUUM_INTERVAL
        );
        assert_eq!(
            delta_vacuum_interval_from_value(Some("")),
            DEFAULT_DELTA_VACUUM_INTERVAL
        );
        assert_eq!(delta_vacuum_interval_from_value(Some("10")), 10);
        assert_eq!(delta_vacuum_interval_from_value(Some(" 10 ")), 10);
        // 0 disables periodic compaction (honored, not defaulted).
        assert_eq!(delta_vacuum_interval_from_value(Some("0")), 0);
        // Garbage falls back to the default.
        assert_eq!(
            delta_vacuum_interval_from_value(Some("nope")),
            DEFAULT_DELTA_VACUUM_INTERVAL
        );
    }

    #[test]
    fn cold_build_resets_delta_counter_to_zero() {
        let store = CouplingStore::open_in_memory().unwrap();
        // Simulate prior deltas having advanced the counter.
        store.reset_delta_since_vacuum().unwrap();
        {
            let conn = store.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO coupling_meta (key, value) VALUES (?1, '42')
                 ON CONFLICT(key) DO UPDATE SET value = '42'",
                params![META_DELTA_SINCE_VACUUM],
            )
            .unwrap();
        }
        assert_eq!(store.delta_since_vacuum().unwrap(), 42);

        store
            .commit_cold_build(
                &[sample_row("a.rs", "b.rs", 1, 1.0, 100)],
                &[("oid1".to_string(), 100)],
                &[ledger("oid1", "a.rs", "b.rs", 100)],
                Some("head1"),
                100,
            )
            .unwrap();

        assert_eq!(
            store.delta_since_vacuum().unwrap(),
            0,
            "cold build must reset the delta-since-vacuum counter"
        );
    }

    #[test]
    fn commit_delta_increments_counter_without_vacuum_below_interval() {
        let _lock = VACUUM_ENV_LOCK.lock().unwrap();
        set_vacuum_env("100"); // high interval — no VACUUM during this test
        let store = CouplingStore::open_in_memory().unwrap();

        store
            .commit_cold_build(&[], &[], &[], Some("head0"), 100)
            .unwrap();
        assert_eq!(store.delta_since_vacuum().unwrap(), 0);

        for i in 1..=3 {
            store
                .commit_delta(
                    &[(format!("oid{i}"), 100 + i)],
                    &[ledger(&format!("oid{i}"), "a.rs", "b.rs", 100 + i)],
                    &[],
                    Some(&format!("head{i}")),
                    Some(100),
                    100 + i,
                    1_000_000,
                )
                .unwrap();
            assert_eq!(
                store.delta_since_vacuum().unwrap(),
                i as u64,
                "counter advances once per delta"
            );
        }
        clear_vacuum_env();
    }

    #[test]
    fn periodic_vacuum_resets_counter_and_shrinks_file() {
        let _lock = VACUUM_ENV_LOCK.lock().unwrap();
        set_vacuum_env("2"); // VACUUM when counter reaches 2
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("coupling.db");
        let store = CouplingStore::open(&db_path).unwrap();

        // Seed a large aggregate, then churn it to create free pages so the
        // VACUUM has something measurable to reclaim. Cold build VACUUMs, so
        // start the counter clean afterwards.
        let mut rows = Vec::new();
        for i in 0..4000 {
            rows.push(sample_row(
                &format!("file_{i}.rs"),
                &format!("file_{}.rs", i + 1),
                1,
                1.0,
                100,
            ));
        }
        store
            .commit_cold_build(&rows, &[("oid0".to_string(), 100)], &[], Some("head0"), 100)
            .unwrap();
        assert_eq!(store.delta_since_vacuum().unwrap(), 0);

        // First delta: counter -> 1, below interval, no VACUUM yet.
        store
            .commit_delta(
                &[("oid1".to_string(), 101)],
                &[ledger("oid1", "a.rs", "b.rs", 101)],
                &[],
                Some("head1"),
                Some(100),
                101,
                1_000_000,
            )
            .unwrap();
        assert_eq!(store.delta_since_vacuum().unwrap(), 1);

        // Manually bloat the file: delete the bulk rows (frees pages) just
        // before the delta that crosses the interval, so VACUUM reclaims them.
        {
            let conn = store.conn.lock().unwrap();
            conn.execute("DELETE FROM coupling", []).unwrap();
        }
        let size_before = std::fs::metadata(&db_path).unwrap().len();

        // Second delta: counter -> 2, reaches interval, triggers VACUUM+reset.
        store
            .commit_delta(
                &[("oid2".to_string(), 102)],
                &[ledger("oid2", "a.rs", "b.rs", 102)],
                &[],
                Some("head2"),
                Some(100),
                102,
                1_000_000,
            )
            .unwrap();

        assert_eq!(
            store.delta_since_vacuum().unwrap(),
            0,
            "reaching the interval must VACUUM and reset the counter"
        );
        let size_after = std::fs::metadata(&db_path).unwrap().len();
        assert!(
            size_after < size_before,
            "VACUUM should shrink the file: before={size_before}, after={size_after}"
        );
        clear_vacuum_env();
    }

    #[test]
    fn periodic_vacuum_disabled_when_interval_zero() {
        let _lock = VACUUM_ENV_LOCK.lock().unwrap();
        set_vacuum_env("0"); // disabled
        let store = CouplingStore::open_in_memory().unwrap();
        store
            .commit_cold_build(&[], &[], &[], Some("head0"), 100)
            .unwrap();

        for i in 1..=5 {
            store
                .commit_delta(
                    &[(format!("oid{i}"), 100 + i)],
                    &[ledger(&format!("oid{i}"), "a.rs", "b.rs", 100 + i)],
                    &[],
                    Some(&format!("head{i}")),
                    Some(100),
                    100 + i,
                    1_000_000,
                )
                .unwrap();
        }
        // Counter keeps climbing because compaction is disabled; it is never
        // reset by a VACUUM.
        assert_eq!(
            store.delta_since_vacuum().unwrap(),
            5,
            "interval=0 disables periodic VACUUM, so the counter is never reset"
        );
        clear_vacuum_env();
    }
}
