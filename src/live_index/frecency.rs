//! Per-workspace frecency scoring for file ranking.
//!
//! Two complementary layers live here:
//!
//! 1. [`FrecencyStore`] — SQLite-backed or in-memory, bumped on commitment
//!    tools (reads/edits of known files) and never on discovery tools
//!    (search). Decays on a 7-day half-life. This is the storage + scoring
//!    layer.
//! 2. [`bump`] — the call-site façade that commitment tools
//!    (`get_file_context`, `get_file_content`, `get_symbol`,
//!    `get_symbol_context`, the seven edit tools) invoke at the end of their
//!    happy path. Unset `SYMFORGE_FRECENCY` collects session-scoped in-memory
//!    history; `SYMFORGE_FRECENCY="1"` keeps the existing persistent SQLite
//!    collection; explicit false/off/disabled values disable collection.
//!    Infallible — every error is silently dropped so the feature cannot break
//!    the tools it hooks into.
//!
//! Discovery tools (`search_files`, `search_text`, `search_symbols`)
//! deliberately never call [`bump`] — see the spec §"Search tools deliberately
//! do NOT bump" for the positive-feedback-loop rationale.
//!
//! Spec: `wiki/concepts/SymForge Frecency-Weighted File Ranking.md`.

use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// Half-life for frecency decay, in seconds. 7 days.
pub const HALF_LIFE_SECS: i64 = 7 * 24 * 60 * 60;

/// Commit-distance thresholds for the graduated HEAD-change reset policy.
pub const RESET_NOOP_THRESHOLD: u32 = 50;
pub const RESET_HALVE_THRESHOLD: u32 = 500;

const META_LAST_HEAD: &str = "last_head_sha";

/// Env var that controls frecency collection policy. Unset means session
/// collection, `"1"` means persistent collection, and false/off/disabled
/// values disable collection.
pub const FRECENCY_FLAG_ENV: &str = "SYMFORGE_FRECENCY";

/// Outcome of applying the HEAD-change reset policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetOutcome {
    /// No action taken (first session, same HEAD, or distance below threshold).
    NoOp,
    /// All `hit_count` values were halved.
    Halved,
    /// All `hit_count` values were zeroed.
    Zeroed,
}

/// A single frecency row as surfaced by `last_10_bumps` in health output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BumpEntry {
    pub path: PathBuf,
    pub last_access_ts: i64,
    pub hit_count: i64,
}

/// Snapshot of frecency scores available to a call-time ranking request.
#[derive(Debug, Clone)]
pub struct FrecencyRankingSnapshot {
    pub scores: HashMap<PathBuf, f64>,
    pub source: String,
}

/// SQLite-backed per-workspace frecency store.
///
/// Persists to `.symforge/frecency.db` (built via [`frecency_db_path`])
/// or an in-memory DB for tests. All access routes through an internal `Mutex`
/// so the store is `Sync` and safe to share across concurrent bump callers.
pub struct FrecencyStore {
    conn: Mutex<Connection>,
}

impl FrecencyStore {
    /// Open a file-backed store, creating the DB and parent directory if missing.
    ///
    /// Sets `busy_timeout = 5s` as a circuit-breaker for cross-process contention
    /// (e.g. a parallel `symforge` subagent on the same workspace). Same-process
    /// contention is handled at a higher layer by the cached store registry, so
    /// in-process callers should never exercise this fallback.
    pub fn open(db_path: &Path) -> rusqlite::Result<Self> {
        if let Some(parent) = db_path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .map_err(|_| rusqlite::Error::InvalidPath(parent.to_path_buf()))?;
        }
        let conn = Connection::open(db_path)?;
        // Best-effort WAL; silently falls back on in-memory/read-only FS.
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        let _ = conn.busy_timeout(std::time::Duration::from_secs(5));
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.migrate()?;
        Ok(store)
    }

    /// Open an existing file-backed store for read-only discovery use.
    ///
    /// Unlike [`Self::open`], this never creates parent directories, a database
    /// file, or schema. Search tools can use it to consume frecency scores
    /// without leaving a frecency footprint in discovery-only sessions.
    pub fn open_existing_readonly(db_path: &Path) -> rusqlite::Result<Option<Self>> {
        if !db_path.exists() {
            return Ok(None);
        }
        let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let _ = conn.busy_timeout(std::time::Duration::from_secs(5));
        Ok(Some(Self {
            conn: Mutex::new(conn),
        }))
    }

    /// Open an in-memory store. For tests and ephemeral use.
    pub fn open_in_memory() -> rusqlite::Result<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> rusqlite::Result<()> {
        let conn = self.conn.lock().expect("frecency mutex poisoned");
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS frecency (
                path TEXT PRIMARY KEY,
                last_access_ts INTEGER NOT NULL,
                hit_count INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
             );",
        )?;
        Ok(())
    }

    /// Bump the given paths at `now_ts`. Each path increments `hit_count` by 1
    /// and sets `last_access_ts = now_ts`. The caller is responsible for
    /// deduplicating within a single invocation (per the Implementation Notes
    /// §"Bump dedup per tool invocation").
    pub fn bump(&self, paths: &[PathBuf], now_ts: i64) -> rusqlite::Result<()> {
        if paths.is_empty() {
            return Ok(());
        }
        let mut conn = self.conn.lock().expect("frecency mutex poisoned");
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO frecency(path, last_access_ts, hit_count)
                 VALUES (?1, ?2, 1)
                 ON CONFLICT(path) DO UPDATE SET
                    last_access_ts = excluded.last_access_ts,
                    hit_count = frecency.hit_count + 1",
            )?;
            for p in paths {
                stmt.execute(params![normalize_path(p), now_ts])?;
            }
        }
        tx.commit()
    }

    /// Decayed frecency score for a single path. Missing paths return `0.0`.
    pub fn score(&self, path: &Path, now_ts: i64) -> rusqlite::Result<f64> {
        let conn = self.conn.lock().expect("frecency mutex poisoned");
        let row: Option<(i64, i64)> = conn
            .query_row(
                "SELECT last_access_ts, hit_count FROM frecency WHERE path = ?1",
                params![normalize_path(path)],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        Ok(row
            .map(|(ts, hits)| decay_score(hits, now_ts, ts))
            .unwrap_or(0.0))
    }

    /// Batch-score many paths at once. Missing paths are omitted from the map.
    pub fn bulk_scores(
        &self,
        paths: &[&Path],
        now_ts: i64,
    ) -> rusqlite::Result<HashMap<PathBuf, f64>> {
        let mut out = HashMap::with_capacity(paths.len());
        if paths.is_empty() {
            return Ok(out);
        }
        let conn = self.conn.lock().expect("frecency mutex poisoned");
        let mut stmt =
            conn.prepare_cached("SELECT last_access_ts, hit_count FROM frecency WHERE path = ?1")?;
        for p in paths {
            let key = normalize_path(p);
            let row: Option<(i64, i64)> = stmt
                .query_row(params![key], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()?;
            if let Some((ts, hits)) = row {
                out.insert(PathBuf::from(&key), decay_score(hits, now_ts, ts));
            }
        }
        Ok(out)
    }

    /// Most-recently-bumped rows, newest first, capped at 10. For health output.
    pub fn last_10_bumps(&self) -> rusqlite::Result<Vec<BumpEntry>> {
        let conn = self.conn.lock().expect("frecency mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT path, last_access_ts, hit_count FROM frecency
             ORDER BY last_access_ts DESC LIMIT 10",
        )?;
        stmt.query_map([], |r| {
            Ok(BumpEntry {
                path: PathBuf::from(r.get::<_, String>(0)?),
                last_access_ts: r.get(1)?,
                hit_count: r.get(2)?,
            })
        })?
        .collect()
    }

    /// Top-N paths ordered by decayed score at `now_ts`. For health output.
    pub fn top_frecent(&self, n: usize, now_ts: i64) -> rusqlite::Result<Vec<(PathBuf, f64)>> {
        let conn = self.conn.lock().expect("frecency mutex poisoned");
        let mut stmt = conn.prepare("SELECT path, last_access_ts, hit_count FROM frecency")?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    PathBuf::from(r.get::<_, String>(0)?),
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut scored: Vec<_> = rows
            .into_iter()
            .map(|(p, ts, hits)| (p, decay_score(hits, now_ts, ts)))
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(n);
        Ok(scored)
    }

    /// Apply the graduated HEAD-change reset policy and persist `current_head`.
    ///
    /// Policy (Implementation Notes §"Reset-on-HEAD-change: graduated, not binary"):
    /// - `last_head` is `None` (first session) → no-op.
    /// - `current_head == last_head` → no-op.
    /// - `commit_distance == None` (unrelated history / branch change) → zero.
    /// - `commit_distance < 50` → no-op.
    /// - `50 <= commit_distance <= 500` → halve all `hit_count`.
    /// - `commit_distance > 500` → zero all `hit_count`.
    ///
    /// The stored `last_head` is updated to `current_head` in every outcome so
    /// subsequent sessions compare against the most recent reset point.
    ///
    /// Note: the `commit_distance` parameter is `Option<u32>` (not the `u32`
    /// the todo text specified). The spec requires "branch change → zero",
    /// which the `git::commit_distance` helper already signals by returning
    /// `Ok(None)` for unrelated histories. Flowing that through preserves the
    /// distinction without an out-of-band sentinel value.
    pub fn reset_or_halve_on_head_change(
        &self,
        last_head: Option<&str>,
        current_head: &str,
        commit_distance: Option<u32>,
    ) -> rusqlite::Result<ResetOutcome> {
        let mut conn = self.conn.lock().expect("frecency mutex poisoned");
        let tx = conn.transaction()?;
        let outcome = match (last_head, commit_distance) {
            (None, _) => ResetOutcome::NoOp,
            (Some(last), _) if last == current_head => ResetOutcome::NoOp,
            (Some(_), None) => {
                tx.execute("UPDATE frecency SET hit_count = 0", [])?;
                ResetOutcome::Zeroed
            }
            (Some(_), Some(d)) if d < RESET_NOOP_THRESHOLD => ResetOutcome::NoOp,
            (Some(_), Some(d)) if d <= RESET_HALVE_THRESHOLD => {
                tx.execute("UPDATE frecency SET hit_count = hit_count / 2", [])?;
                ResetOutcome::Halved
            }
            (Some(_), Some(_)) => {
                tx.execute("UPDATE frecency SET hit_count = 0", [])?;
                ResetOutcome::Zeroed
            }
        };
        tx.execute(
            "INSERT INTO meta(key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![META_LAST_HEAD, current_head],
        )?;
        tx.commit()?;
        Ok(outcome)
    }

    /// Apply the graduated HEAD-change reset policy against `repo_root`'s git HEAD.
    ///
    /// Wraps `last_head` lookup + git HEAD resolution + commit-distance computation
    /// around [`reset_or_halve_on_head_change`]. Used by both the explicit boot-time
    /// `init_frecency_store` path and the lazy first-bump cache-miss path, so the
    /// policy applies wherever the store is first opened for a session.
    ///
    /// All errors are mapped to `String` and any transient git failure (no repo,
    /// detached HEAD, etc.) is silently dropped so the feature never breaks the
    /// tool it hooks into.
    pub fn apply_head_reset_policy(&self, repo_root: &Path) -> Result<(), String> {
        let current_head = match crate::git::head_sha(repo_root) {
            Ok(s) => s,
            Err(_) => return Ok(()),
        };
        let stored_head = self.last_head().map_err(|e| e.to_string())?;
        let distance = match stored_head.as_deref() {
            Some(prev) if prev != current_head => {
                match crate::git::commit_distance(prev, &current_head, repo_root) {
                    Ok(opt) => opt,
                    Err(_) => return Ok(()),
                }
            }
            _ => None,
        };
        self.reset_or_halve_on_head_change(stored_head.as_deref(), &current_head, distance)
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Read the last HEAD SHA this store recorded.
    pub fn last_head(&self) -> rusqlite::Result<Option<String>> {
        let conn = self.conn.lock().expect("frecency mutex poisoned");
        conn.query_row(
            "SELECT value FROM meta WHERE key = ?1",
            params![META_LAST_HEAD],
            |r| r.get::<_, String>(0),
        )
        .optional()
    }
}

/// Decay formula: `hit_count * exp(-ln(2) * (now - last) / HALF_LIFE_SECS)`.
/// Clamps a future `last_ts` (clock skew) to "no decay" rather than amplifying.
#[inline]
fn decay_score(hit_count: i64, now_ts: i64, last_ts: i64) -> f64 {
    let dt = (now_ts - last_ts).max(0) as f64;
    (hit_count as f64) * (-std::f64::consts::LN_2 * dt / HALF_LIFE_SECS as f64).exp()
}

/// Normalize paths to forward-slash form so Windows and Unix key the same row.
/// Mirrors the pattern in `src/git.rs::collect_diff_paths`.
fn normalize_path(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

// ---------------------------------------------------------------------------
// Call-site bump façade
// ---------------------------------------------------------------------------
//
// `bump(repo_root, paths)` is the stable surface commitment-tool handlers call
// at the end of their happy path. It writes either to a session in-memory store
// or to the persistent per-workspace SQLite store depending on policy.

/// Resolve frecency collection policy from the process environment.
///
/// Environment variables are policy/default knobs. Unset means lightweight
/// session collection; `"1"`/truthy/persistent values mean the existing
/// persistent SQLite store; explicit false/off/disabled values disable both
/// collection and ranking use.
pub fn collection_policy_from_env() -> crate::capability::FrecencyCollectionPolicy {
    use crate::capability::FrecencyCollectionPolicy;

    match std::env::var(FRECENCY_FLAG_ENV) {
        Ok(value) => match value.trim().to_ascii_lowercase().as_str() {
            "" | "session" => FrecencyCollectionPolicy::Session,
            "1" | "true" | "yes" | "on" | "persistent" => FrecencyCollectionPolicy::Persistent,
            "0" | "false" | "no" | "off" | "disabled" | "disable" => {
                FrecencyCollectionPolicy::Disabled
            }
            _ => FrecencyCollectionPolicy::Disabled,
        },
        Err(std::env::VarError::NotPresent) => FrecencyCollectionPolicy::Session,
        Err(std::env::VarError::NotUnicode(_)) => FrecencyCollectionPolicy::Disabled,
    }
}

/// Record that the given paths were accessed by a commitment tool.
///
/// No-op only when policy disables frecency. Infallible — callers never need
/// to handle errors; failure to record a bump is silently dropped so the
/// feature cannot break the tool it hooks into.
///
/// `paths` is expected to already be deduplicated (batch tools collect into a
/// `HashSet<PathBuf>` before calling). Looks up a process-cached
/// [`FrecencyStore`] keyed by the workspace. Same-process callers serialize
/// through the store's internal connection mutex with no SQLite-level lock
/// contention. Cross-process contention for persistent collection falls back to
/// the 5-second `busy_timeout` set in [`FrecencyStore::open`].
pub fn bump(repo_root: &Path, paths: &[PathBuf]) {
    if paths.is_empty() {
        return;
    }
    let store = match collection_policy_from_env() {
        crate::capability::FrecencyCollectionPolicy::Disabled => return,
        crate::capability::FrecencyCollectionPolicy::Session => session_store_for(repo_root),
        crate::capability::FrecencyCollectionPolicy::Persistent => cached_store_for(repo_root),
    };
    let Some(store) = store else {
        return;
    };
    let now_ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let _ = store.bump(paths, now_ts);
}

/// Return frecency scores for a call-time ranking request without creating any
/// persistent store. Existing persistent history and current-process session
/// history are both considered; session rows override persistent rows for the
/// same path.
///
/// Note: a pre-existing `.symforge/frecency.db` is consulted whenever ranking
/// is not policy-disabled. A workspace previously run under
/// `SYMFORGE_FRECENCY=1` and now under Session policy will still see those
/// stale persistent rows contribute. Operators wanting a clean slate must
/// delete the DB file.
pub fn ranking_scores_for_paths(
    repo_root: &Path,
    paths: &[&Path],
    now_ts: i64,
) -> Result<Option<FrecencyRankingSnapshot>, String> {
    let mut scores = HashMap::new();
    let mut sources = Vec::new();

    if let Some(store) = cached_persistent_for(repo_root) {
        // Reuse the cached writer when it exists. The `bulk_scores` SQL
        // executes under the same `FrecencyStore::conn` mutex as bumps and
        // HEAD-reset, so SQL execution is serialized. Snapshot semantics are
        // not — a HEAD-reset that commits between the cache lookup and the
        // SQL read is reflected in the returned scores.
        scores.extend(
            store
                .bulk_scores(paths, now_ts)
                .map_err(|err| err.to_string())?,
        );
        sources.push("persistent (cached)");
    } else {
        let db_path = frecency_db_path(repo_root);
        match FrecencyStore::open_existing_readonly(&db_path) {
            Ok(Some(store)) => {
                scores.extend(
                    store
                        .bulk_scores(paths, now_ts)
                        .map_err(|err| err.to_string())?,
                );
                sources.push("persistent");
            }
            Ok(None) => {}
            Err(err) => return Err(err.to_string()),
        }
    }

    if let Some(store) = cached_session_store_for(repo_root) {
        scores.extend(
            store
                .bulk_scores(paths, now_ts)
                .map_err(|err| err.to_string())?,
        );
        sources.push("session");
    }

    if sources.is_empty() {
        return Ok(None);
    }

    Ok(Some(FrecencyRankingSnapshot {
        scores,
        source: format!("{} frecency history", sources.join(" + ")),
    }))
}

fn persistent_cache() -> &'static Mutex<HashMap<PathBuf, Arc<FrecencyStore>>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<FrecencyStore>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The on-disk frecency db path for `repo_root`: `repo_root/.symforge/frecency.db`.
///
/// The single construction point for the frecency db path. Routes through
/// [`crate::paths::symforge_db_path`] (the lone `.symforge` prefix owner) so the
/// path can never be hand-rolled and doubled (D1-ROOT). All frecency call sites
/// — cache key, readonly open, init, health probe — go through here so they all
/// agree byte-for-byte.
pub(crate) fn frecency_db_path(repo_root: &Path) -> PathBuf {
    crate::paths::symforge_db_path(repo_root, crate::paths::FRECENCY_DB_NAME)
}

/// Look up (or lazily create) the cached [`FrecencyStore`] for `repo_root`.
///
/// All same-process callers for a given workspace share the same `Arc` and
/// thus the same connection mutex. Returns `None` if the store cannot be
/// opened — bump is infallible at the call site, so we just drop the bump.
///
/// First cache-miss per repo applies the HEAD-change reset policy before
/// inserting into the cache. In the persistent lazy path, commitment-tool bumps
/// are the trigger for the policy. Startup initialization can still warm this
/// path when policy explicitly requests persistent collection. The reset call
/// happens INSIDE the cache mutex so two parallel bumps cannot race on policy
/// application.
fn cached_store_for(repo_root: &Path) -> Option<std::sync::Arc<FrecencyStore>> {
    let cache = persistent_cache();
    let key = frecency_db_path(repo_root);
    let mut guard = cache.lock().ok()?;
    if let Some(existing) = guard.get(&key) {
        return Some(Arc::clone(existing));
    }
    let store = Arc::new(FrecencyStore::open(&key).ok()?);
    // Apply HEAD-change reset on first open per repo per process. Errors are
    // silently dropped: a transient git failure must not break the bump path.
    let _ = store.apply_head_reset_policy(repo_root);
    guard.insert(key, Arc::clone(&store));
    Some(store)
}

fn cached_persistent_for(repo_root: &Path) -> Option<Arc<FrecencyStore>> {
    let cache = persistent_cache();
    let key = frecency_db_path(repo_root);
    let guard = cache.lock().ok()?;
    guard.get(&key).map(Arc::clone)
}

fn session_cache() -> &'static Mutex<HashMap<PathBuf, Arc<FrecencyStore>>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<FrecencyStore>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn session_store_for(repo_root: &Path) -> Option<Arc<FrecencyStore>> {
    let cache = session_cache();
    let key = repo_root.to_path_buf();
    let mut guard = cache.lock().ok()?;
    if let Some(existing) = guard.get(&key) {
        return Some(Arc::clone(existing));
    }
    let store = Arc::new(FrecencyStore::open_in_memory().ok()?);
    guard.insert(key, Arc::clone(&store));
    Some(store)
}

fn cached_session_store_for(repo_root: &Path) -> Option<Arc<FrecencyStore>> {
    let cache = session_cache();
    let guard = cache.lock().ok()?;
    guard.get(repo_root).map(Arc::clone)
}

/// [`EditHook`] implementation that records a frecency bump after every
/// successful edit commit. Delegates to [`bump`], so the
/// collection policy check happens there — registering the hook is itself
/// unconditional.
#[cfg(feature = "server")]
pub struct FrecencyBumpHook;

#[cfg(feature = "server")]
impl crate::protocol::edit_hooks::EditHook for FrecencyBumpHook {
    fn after_edit_committed(
        &self,
        ctx: &crate::protocol::edit_hooks::EditContext,
        _resolved_path: &Path,
    ) {
        bump(ctx.repo_root, &[PathBuf::from(ctx.relative_path)]);
    }
}

/// Register [`FrecencyBumpHook`] on the process-wide edit-hook registry exactly
/// once. Safe to call from every `LiveIndex::load` — the inner [`OnceLock`]
/// dedupes. The hook body resolves collection policy at call time, so
/// registering unconditionally is cheap and keeps env changes observable after
/// startup.
pub fn ensure_bump_hook_registered() {
    // Server builds register a frecency-bump hook on the protocol edit registry.
    // `embed` builds have no protocol edit registry (embedders drive the engine
    // directly), so this is a no-op there.
    #[cfg(feature = "server")]
    {
        use std::sync::OnceLock;
        static REGISTERED: OnceLock<()> = OnceLock::new();
        REGISTERED.get_or_init(|| {
            crate::protocol::edit_hooks::register(Box::new(FrecencyBumpHook));
        });
    }
}

/// Drain and return every path recorded by [`bump`] since the last drain/clear.
///
/// Intended for wiring tests that need to observe whether a tool handler
/// actually called [`bump`]. Kept `pub` (behind `#[doc(hidden)]`) because the
/// integration-test crate lives outside the library crate and cannot use
/// `#[cfg(test)]`-gated items.
#[doc(hidden)]
/// Clear the test-observability sink without returning its contents.
#[doc(hidden)]
#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------
    // FrecencyStore tests (storage layer — from swarm-1)
    // -----------------------------------------------------------------

    fn make_store() -> FrecencyStore {
        FrecencyStore::open_in_memory().expect("open in-memory frecency store")
    }

    fn norm(p: &Path) -> PathBuf {
        PathBuf::from(normalize_path(p))
    }

    #[test]
    fn bump_inserts_new_path_with_hit_count_one() {
        let store = make_store();
        let p = PathBuf::from("src/foo.rs");
        store.bump(std::slice::from_ref(&p), 1_000).unwrap();
        assert_eq!(store.score(&p, 1_000).unwrap(), 1.0);
    }

    #[test]
    fn bump_increments_existing_path() {
        let store = make_store();
        let p = PathBuf::from("src/foo.rs");
        store.bump(std::slice::from_ref(&p), 1_000).unwrap();
        store.bump(std::slice::from_ref(&p), 2_000).unwrap();
        store.bump(std::slice::from_ref(&p), 3_000).unwrap();
        assert_eq!(store.score(&p, 3_000).unwrap(), 3.0);
    }

    #[test]
    fn bump_updates_last_access_ts() {
        let store = make_store();
        let p = PathBuf::from("src/foo.rs");
        store.bump(std::slice::from_ref(&p), 1_000).unwrap();
        store.bump(std::slice::from_ref(&p), 5_000).unwrap();
        let entries = store.last_10_bumps().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].last_access_ts, 5_000);
        assert_eq!(entries[0].hit_count, 2);
    }

    #[test]
    fn bump_multiple_paths_in_single_call() {
        let store = make_store();
        let paths = vec![
            PathBuf::from("src/a.rs"),
            PathBuf::from("src/b.rs"),
            PathBuf::from("src/c.rs"),
        ];
        store.bump(&paths, 1_000).unwrap();
        for p in &paths {
            assert_eq!(store.score(p, 1_000).unwrap(), 1.0);
        }
    }

    #[test]
    fn empty_bump_is_noop() {
        let store = make_store();
        store.bump(&[], 0).unwrap();
        assert!(store.top_frecent(10, 0).unwrap().is_empty());
    }

    #[test]
    fn score_returns_zero_for_missing_path() {
        let store = make_store();
        assert_eq!(store.score(Path::new("nope.rs"), 1_000).unwrap(), 0.0);
    }

    #[test]
    fn score_decays_by_half_at_one_half_life() {
        let store = make_store();
        let p = PathBuf::from("src/foo.rs");
        store.bump(std::slice::from_ref(&p), 0).unwrap();
        let score = store.score(&p, HALF_LIFE_SECS).unwrap();
        assert!(
            (score - 0.5).abs() < 1e-9,
            "expected ~0.5 at 1 half-life, got {score}"
        );
    }

    #[test]
    fn score_decays_to_quarter_at_two_half_lives() {
        let store = make_store();
        let p = PathBuf::from("src/foo.rs");
        store.bump(std::slice::from_ref(&p), 0).unwrap();
        let score = store.score(&p, HALF_LIFE_SECS * 2).unwrap();
        assert!(
            (score - 0.25).abs() < 1e-9,
            "expected ~0.25 at 2 half-lives, got {score}"
        );
    }

    #[test]
    fn score_is_stable_when_now_equals_last_access() {
        let store = make_store();
        let p = PathBuf::from("src/foo.rs");
        store.bump(std::slice::from_ref(&p), 12_345).unwrap();
        assert_eq!(store.score(&p, 12_345).unwrap(), 1.0);
    }

    #[test]
    fn score_does_not_amplify_on_clock_skew() {
        // If now_ts < last_access_ts (clock skew), score should not exceed hit_count.
        let store = make_store();
        let p = PathBuf::from("src/foo.rs");
        store.bump(std::slice::from_ref(&p), 10_000).unwrap();
        assert_eq!(store.score(&p, 9_000).unwrap(), 1.0);
    }

    #[test]
    fn fusion_property_recent_outranks_ancient_with_many_hits() {
        // "File touched 5 min ago outranks file touched 6 months ago with 10× hits."
        let store = make_store();
        let ancient = PathBuf::from("src/ancient.rs");
        let recent = PathBuf::from("src/recent.rs");
        let six_months: i64 = 60 * 60 * 24 * 30 * 6;
        for _ in 0..10 {
            store.bump(std::slice::from_ref(&ancient), 0).unwrap();
        }
        store
            .bump(std::slice::from_ref(&recent), six_months - 300)
            .unwrap();
        let now = six_months;
        assert!(store.score(&recent, now).unwrap() > store.score(&ancient, now).unwrap());
    }

    #[test]
    fn bulk_scores_matches_per_path_score() {
        let store = make_store();
        let a = PathBuf::from("src/a.rs");
        let b = PathBuf::from("src/b.rs");
        let missing = PathBuf::from("src/missing.rs");
        store.bump(std::slice::from_ref(&a), 0).unwrap();
        store.bump(std::slice::from_ref(&b), 0).unwrap();
        store.bump(std::slice::from_ref(&b), 0).unwrap();
        let now = HALF_LIFE_SECS;
        let paths: Vec<&Path> = vec![a.as_path(), b.as_path(), missing.as_path()];
        let bulk = store.bulk_scores(&paths, now).unwrap();
        assert_eq!(bulk.len(), 2, "missing path must be omitted from bulk map");
        assert!((bulk[&norm(&a)] - store.score(&a, now).unwrap()).abs() < 1e-9);
        assert!((bulk[&norm(&b)] - store.score(&b, now).unwrap()).abs() < 1e-9);
    }

    #[test]
    fn bulk_scores_empty_input_is_empty_map() {
        let store = make_store();
        let paths: Vec<&Path> = vec![];
        assert!(store.bulk_scores(&paths, 0).unwrap().is_empty());
    }

    #[test]
    fn last_10_bumps_returns_most_recent_first() {
        let store = make_store();
        for i in 0..15 {
            store
                .bump(&[PathBuf::from(format!("src/f_{i}.rs"))], i as i64 * 1_000)
                .unwrap();
        }
        let entries = store.last_10_bumps().unwrap();
        assert_eq!(entries.len(), 10);
        assert_eq!(entries[0].last_access_ts, 14_000);
        assert_eq!(entries[9].last_access_ts, 5_000);
    }

    #[test]
    fn top_frecent_orders_by_decayed_score() {
        let store = make_store();
        let hot = PathBuf::from("hot.rs");
        let warm = PathBuf::from("warm.rs");
        let cold = PathBuf::from("cold.rs");
        // Same last_access_ts; hit counts differ.
        store.bump(std::slice::from_ref(&hot), 100).unwrap();
        store.bump(std::slice::from_ref(&hot), 100).unwrap();
        store.bump(std::slice::from_ref(&warm), 100).unwrap();
        // Cold: 1 hit, but decayed by 5 half-lives.
        store
            .bump(std::slice::from_ref(&cold), 100 - HALF_LIFE_SECS * 5)
            .unwrap();
        let top = store.top_frecent(3, 100).unwrap();
        assert_eq!(top.len(), 3);
        assert_eq!(top[0].0, hot);
        assert_eq!(top[1].0, warm);
        assert_eq!(top[2].0, cold);
    }

    #[test]
    fn top_frecent_respects_n_limit() {
        let store = make_store();
        for i in 0..20 {
            store.bump(&[PathBuf::from(format!("f{i}.rs"))], 0).unwrap();
        }
        assert_eq!(store.top_frecent(5, 0).unwrap().len(), 5);
    }

    #[test]
    fn reset_first_session_noops_and_stores_head() {
        let store = make_store();
        let p = PathBuf::from("src/foo.rs");
        store.bump(std::slice::from_ref(&p), 0).unwrap();
        let outcome = store
            .reset_or_halve_on_head_change(None, "abc123", Some(1_000))
            .unwrap();
        assert_eq!(outcome, ResetOutcome::NoOp);
        assert_eq!(store.score(&p, 0).unwrap(), 1.0);
        assert_eq!(store.last_head().unwrap().as_deref(), Some("abc123"));
    }

    #[test]
    fn reset_same_head_noops() {
        let store = make_store();
        let p = PathBuf::from("src/foo.rs");
        store.bump(std::slice::from_ref(&p), 0).unwrap();
        let outcome = store
            .reset_or_halve_on_head_change(Some("sha"), "sha", Some(0))
            .unwrap();
        assert_eq!(outcome, ResetOutcome::NoOp);
        assert_eq!(store.score(&p, 0).unwrap(), 1.0);
    }

    #[test]
    fn reset_below_50_commits_noops() {
        let store = make_store();
        let p = PathBuf::from("src/foo.rs");
        for _ in 0..4 {
            store.bump(std::slice::from_ref(&p), 0).unwrap();
        }
        let outcome = store
            .reset_or_halve_on_head_change(Some("old"), "new", Some(49))
            .unwrap();
        assert_eq!(outcome, ResetOutcome::NoOp);
        assert_eq!(store.score(&p, 0).unwrap(), 4.0);
    }

    #[test]
    fn reset_at_50_halves_hits() {
        let store = make_store();
        let p = PathBuf::from("src/foo.rs");
        for _ in 0..10 {
            store.bump(std::slice::from_ref(&p), 0).unwrap();
        }
        let outcome = store
            .reset_or_halve_on_head_change(Some("old"), "new", Some(50))
            .unwrap();
        assert_eq!(outcome, ResetOutcome::Halved);
        assert_eq!(store.score(&p, 0).unwrap(), 5.0);
    }

    #[test]
    fn reset_at_500_halves_hits() {
        let store = make_store();
        let p = PathBuf::from("src/foo.rs");
        for _ in 0..10 {
            store.bump(std::slice::from_ref(&p), 0).unwrap();
        }
        let outcome = store
            .reset_or_halve_on_head_change(Some("old"), "new", Some(500))
            .unwrap();
        assert_eq!(outcome, ResetOutcome::Halved);
        assert_eq!(store.score(&p, 0).unwrap(), 5.0);
    }

    #[test]
    fn reset_above_500_zeros_hits() {
        let store = make_store();
        let p = PathBuf::from("src/foo.rs");
        for _ in 0..10 {
            store.bump(std::slice::from_ref(&p), 0).unwrap();
        }
        let outcome = store
            .reset_or_halve_on_head_change(Some("old"), "new", Some(501))
            .unwrap();
        assert_eq!(outcome, ResetOutcome::Zeroed);
        assert_eq!(store.score(&p, 0).unwrap(), 0.0);
    }

    #[test]
    fn reset_unrelated_history_zeros_hits() {
        let store = make_store();
        let p = PathBuf::from("src/foo.rs");
        for _ in 0..10 {
            store.bump(std::slice::from_ref(&p), 0).unwrap();
        }
        let outcome = store
            .reset_or_halve_on_head_change(Some("old"), "new", None)
            .unwrap();
        assert_eq!(outcome, ResetOutcome::Zeroed);
        assert_eq!(store.score(&p, 0).unwrap(), 0.0);
    }

    #[test]
    fn reset_updates_stored_head_across_outcomes() {
        let store = make_store();
        store.bump(&[PathBuf::from("src/foo.rs")], 0).unwrap();
        store
            .reset_or_halve_on_head_change(Some("a"), "b", Some(10))
            .unwrap();
        assert_eq!(store.last_head().unwrap().as_deref(), Some("b"));
        store
            .reset_or_halve_on_head_change(Some("b"), "c", Some(200))
            .unwrap();
        assert_eq!(store.last_head().unwrap().as_deref(), Some("c"));
        store
            .reset_or_halve_on_head_change(Some("c"), "d", Some(10_000))
            .unwrap();
        assert_eq!(store.last_head().unwrap().as_deref(), Some("d"));
    }

    #[test]
    fn path_normalization_treats_backslash_and_forward_slash_as_same_row() {
        let store = make_store();
        let windows = PathBuf::from("src\\foo.rs");
        let unix = PathBuf::from("src/foo.rs");
        store.bump(&[windows], 0).unwrap();
        store.bump(std::slice::from_ref(&unix), 1_000).unwrap();
        assert_eq!(store.score(&unix, 1_000).unwrap(), 2.0);
        assert_eq!(store.last_10_bumps().unwrap().len(), 1);
    }

    #[test]
    fn open_file_backed_creates_db_and_parent_dir_and_persists() {
        let tmp = tempfile::TempDir::new().unwrap();
        let nested = tmp.path().join("nested").join("frecency.db");
        {
            let store = FrecencyStore::open(&nested).unwrap();
            store.bump(&[PathBuf::from("src/foo.rs")], 0).unwrap();
        }
        assert!(nested.exists(), "db file should be created");
        let store2 = FrecencyStore::open(&nested).unwrap();
        assert_eq!(store2.score(Path::new("src/foo.rs"), 0).unwrap(), 1.0);
    }

    // -----------------------------------------------------------------
    // Call-site bump() façade tests
    //
    // The façade now opens the per-workspace SQLite store on demand and
    // writes directly. Verifying behavior means querying the store after
    // the call rather than draining a sink. Tests serialize on ENV_LOCK
    // because they mutate `SYMFORGE_FRECENCY`.
    // -----------------------------------------------------------------

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[allow(unsafe_code)] // test-only flag helper runs under ENV_LOCK and --test-threads=1.
    fn set_flag_on() {
        // SAFETY: tests hold ENV_LOCK and run with --test-threads=1; no
        // concurrent env readers can observe the transition.
        unsafe { std::env::set_var(FRECENCY_FLAG_ENV, "1") };
    }

    #[allow(unsafe_code)] // test-only flag helper runs under ENV_LOCK and --test-threads=1.
    fn clear_flag() {
        // SAFETY: see set_flag_on.
        unsafe { std::env::remove_var(FRECENCY_FLAG_ENV) };
    }

    fn db_path_for(root: &Path) -> PathBuf {
        // Route the test path through the production helper so the test exercises
        // the SAME construction production uses (the blind spot that hid D1/D7
        // was a test computing a DIFFERENT path than production).
        super::frecency_db_path(root)
    }

    #[test]
    fn ranking_scores_without_history_repeatedly_stays_footprint_free() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let db_path = db_path_for(tmp.path());
        let db_parent = db_path.parent().expect("frecency db has parent");
        let path = PathBuf::from("src/lib.rs");
        let path_refs = [path.as_path()];

        for _ in 0..3 {
            let snapshot = super::ranking_scores_for_paths(tmp.path(), &path_refs, 0)
                .expect("read-only ranking score lookup should not fail");
            assert!(
                snapshot.is_none(),
                "missing persistent and session history should return no ranking snapshot"
            );
            assert!(
                !db_parent.exists(),
                "read-only ranking score lookup must not create the .symforge directory"
            );
            assert!(
                !db_path.exists(),
                "read-only ranking score lookup must not create frecency.db"
            );
        }
    }

    #[test]
    fn module_bump_records_session_paths_when_flag_unset() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_flag();
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = PathBuf::from("src/lib.rs");
        super::bump(tmp.path(), std::slice::from_ref(&path));
        let path_refs = [path.as_path()];
        let snapshot = super::ranking_scores_for_paths(tmp.path(), &path_refs, 0)
            .expect("session scores ok")
            .expect("session history present");
        assert_eq!(
            snapshot.scores.get(Path::new("src/lib.rs")),
            Some(&1.0),
            "unset policy should collect session frecency"
        );
        assert!(
            !db_path_for(tmp.path()).exists(),
            "session collection must not create the persistent database"
        );
    }

    #[allow(unsafe_code)] // test-only flag mutation runs under ENV_LOCK and --test-threads=1.
    #[test]
    fn module_bump_is_noop_when_flag_not_one() {
        let _g = ENV_LOCK.lock().unwrap();
        // SAFETY: see set_flag_on.
        unsafe { std::env::set_var(FRECENCY_FLAG_ENV, "0") };
        let tmp = tempfile::tempdir().expect("tempdir");
        super::bump(tmp.path(), &[PathBuf::from("src/lib.rs")]);
        clear_flag();
        assert!(
            !db_path_for(tmp.path()).exists(),
            "bump with disabled policy must not touch disk"
        );
    }

    #[test]
    fn module_bump_records_paths_when_flag_on() {
        let _g = ENV_LOCK.lock().unwrap();
        set_flag_on();
        let tmp = tempfile::tempdir().expect("tempdir");
        super::bump(
            tmp.path(),
            &[PathBuf::from("src/a.rs"), PathBuf::from("src/b.rs")],
        );
        clear_flag();
        let store = FrecencyStore::open(&db_path_for(tmp.path())).unwrap();
        let entries = store.last_10_bumps().unwrap();
        let mut paths: Vec<PathBuf> = entries.iter().map(|e| e.path.clone()).collect();
        paths.sort();
        assert_eq!(
            paths,
            vec![PathBuf::from("src/a.rs"), PathBuf::from("src/b.rs")],
            "bump with flag on must persist every supplied path"
        );
    }

    #[test]
    fn module_bump_empty_slice_is_noop_when_flag_on() {
        let _g = ENV_LOCK.lock().unwrap();
        set_flag_on();
        let tmp = tempfile::tempdir().expect("tempdir");
        super::bump(tmp.path(), &[]);
        clear_flag();
        // Empty slice: short-circuit before opening the store.
        assert!(
            !db_path_for(tmp.path()).exists(),
            "empty bump must not touch disk even with flag on"
        );
    }

    #[test]
    fn module_bump_increments_existing_path() {
        let _g = ENV_LOCK.lock().unwrap();
        set_flag_on();
        let tmp = tempfile::tempdir().expect("tempdir");
        super::bump(tmp.path(), &[PathBuf::from("src/lib.rs")]);
        super::bump(tmp.path(), &[PathBuf::from("src/lib.rs")]);
        clear_flag();
        let store = FrecencyStore::open(&db_path_for(tmp.path())).unwrap();
        let entries = store.last_10_bumps().unwrap();
        assert_eq!(entries.len(), 1, "single path collapses into one row");
        assert_eq!(entries[0].hit_count, 2);
    }
}
