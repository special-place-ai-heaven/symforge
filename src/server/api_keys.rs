//! Hashed product API-key store (G-039) for the operator server.
//!
//! Mirrors the rusqlite pattern in [`crate::stel::ledger_store`]:
//! `enum ApiKeyStore { Sqlite(..), Disabled }`, idempotent `migrate()`, an
//! in-memory constructor for tests, and a poisoned-mutex-recovering lock so a
//! prior panic degrades instead of crashing the serve loop (FR-011 discipline).
//!
//! ## Security model (GATE-2 / FR-004 / SC-003)
//!
//! - The **raw** secret is shown **exactly once**, at [`ApiKeyStore::mint`]
//!   time, and is **never** persisted or returned again. Only a SHA-256 hash of
//!   the raw secret is stored.
//! - [`ApiKeyStore::verify`] hashes the presented secret and compares (in
//!   constant time over the hex digest) against the stored hash of every
//!   **active** (non-revoked) key. A revoked key never verifies.
//! - [`ApiKeyStore::list`] returns label / fingerprint / timestamps only —
//!   never the raw secret and never the full hash.
//!
//! The raw secret is a high-entropy 256-bit random bearer token rendered as
//! `sf_<64 hex>`. Because it is random (not a human-chosen password) a plain
//! SHA-256 is appropriate; a slow password-KDF would add a dependency for no
//! security gain on a 256-bit random token (see `research.md` D3/D4).

use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::paths::{API_KEYS_DB_NAME, symforge_db_path};

const CURRENT_SCHEMA_VERSION: u32 = 1;
const META_SCHEMA_VERSION: &str = "schema_version";

const SCHEMA_V1: &str = r#"
CREATE TABLE IF NOT EXISTS api_keys_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS api_keys (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    label        TEXT NOT NULL,
    fingerprint  TEXT NOT NULL,
    hash         TEXT NOT NULL,
    created_ms   INTEGER NOT NULL,
    rotated_ms   INTEGER,
    revoked_ms   INTEGER
);

CREATE INDEX IF NOT EXISTS idx_api_keys_hash ON api_keys (hash);
"#;

const MAX_LABEL_BYTES: usize = 128;

/// A persisted API-key record (never carries the raw secret).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ApiKeyRecord {
    pub id: i64,
    pub label: String,
    /// Short, non-reversible prefix of the hash, safe to display in listings.
    pub fingerprint: String,
    pub created_ms: u64,
    /// Set when the key was rotated (its secret replaced).
    pub rotated_ms: Option<u64>,
    /// Set when the key was revoked; a revoked key never authenticates.
    pub revoked_ms: Option<u64>,
}

impl ApiKeyRecord {
    /// Whether the key is currently usable (not revoked).
    pub fn is_active(&self) -> bool {
        self.revoked_ms.is_none()
    }
}

/// The outcome of a successful [`ApiKeyStore::mint`]: the record plus the raw
/// secret shown **exactly once**.
#[derive(Debug, Clone)]
pub struct MintedKey {
    pub record: ApiKeyRecord,
    /// The raw bearer secret. Present only here, at creation; never stored,
    /// never returned by `list`/`get`. The caller must surface it to the
    /// operator immediately and then drop it.
    pub raw_secret: String,
}

/// Hashed API-key store. `Disabled` when the DB could not open.
#[derive(Debug, Clone)]
pub enum ApiKeyStore {
    Sqlite(SqliteApiKeyStore),
    Disabled,
}

impl ApiKeyStore {
    /// Open or create `api-keys.db` under the project `root`. On any failure
    /// returns `Disabled` (logged, never panics) — the bootstrap `--api-key`
    /// still works regardless (FR-011 / spec edge case).
    ///
    /// `root` is the project ROOT, NOT the `.symforge` data dir. The path is
    /// built through [`symforge_db_path`] (the single `.symforge` prefix owner),
    /// so the db lands at `root/.symforge/api-keys.db`. Before this routed through
    /// the helper, `open` took the already-`.symforge` data dir AND joined a
    /// `.symforge/`-prefixed const, doubling the prefix to
    /// `root/.symforge/.symforge/api-keys.db` (D7, shipped in 8.5.0). Any data at
    /// that pre-fix doubled path is orphaned, not migrated — the store degrades to
    /// `Disabled`/recreates and the bootstrap `--api-key` keeps working, so there
    /// is no data loss for a never-1.0 key store (a stale doubled-path file is
    /// simply unreferenced).
    pub fn open(root: &Path) -> Self {
        let db_path = symforge_db_path(root, API_KEYS_DB_NAME);
        match SqliteApiKeyStore::open(&db_path) {
            Ok(store) => Self::Sqlite(store),
            Err(err) => {
                tracing::warn!(
                    path = %db_path.display(),
                    error = %err,
                    "api-key store failed to open; key management degraded (bootstrap --api-key still works)"
                );
                Self::Disabled
            }
        }
    }

    /// In-memory constructor for tests.
    pub fn open_in_memory() -> Result<Self> {
        Ok(Self::Sqlite(SqliteApiKeyStore::open_in_memory()?))
    }

    /// Whether the store is backed by a working DB.
    pub fn is_enabled(&self) -> bool {
        matches!(self, Self::Sqlite(_))
    }

    /// Mint a new key with `label`. Returns the record and the raw secret
    /// (shown once). Errors only when the store is `Disabled`.
    pub fn mint(&self, label: &str) -> Result<MintedKey> {
        match self {
            Self::Disabled => {
                anyhow::bail!("api-key store unavailable; cannot mint (bootstrap --api-key only)")
            }
            Self::Sqlite(store) => store.mint(label),
        }
    }

    /// List all key records (no raw secret). Empty vec when `Disabled`.
    pub fn list(&self) -> Result<Vec<ApiKeyRecord>> {
        match self {
            Self::Disabled => Ok(vec![]),
            Self::Sqlite(store) => store.list(),
        }
    }

    /// Rotate a key's secret: revokes the old hash, generates a new secret,
    /// stamps `rotated_ms`. Returns the new raw secret (shown once).
    pub fn rotate(&self, id: i64) -> Result<MintedKey> {
        match self {
            Self::Disabled => anyhow::bail!("api-key store unavailable; cannot rotate"),
            Self::Sqlite(store) => store.rotate(id),
        }
    }

    /// Revoke a key. Idempotent: revoking an already-revoked key is a no-op
    /// success. Errors only when the store is `Disabled` or the id is unknown.
    pub fn revoke(&self, id: i64) -> Result<()> {
        match self {
            Self::Disabled => anyhow::bail!("api-key store unavailable; cannot revoke"),
            Self::Sqlite(store) => store.revoke(id),
        }
    }

    /// Verify a presented raw secret against all active (non-revoked) keys.
    /// Returns `false` when `Disabled` (callers fall back to the bootstrap key).
    pub fn verify(&self, presented: &str) -> bool {
        match self {
            Self::Disabled => false,
            Self::Sqlite(store) => store.verify(presented),
        }
    }
}

/// SQLite-backed implementation.
#[derive(Debug, Clone)]
pub struct SqliteApiKeyStore {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteApiKeyStore {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating api-key db parent dir {:?}", parent))?;
        }
        let conn =
            Connection::open(path).with_context(|| format!("opening api-key db at {:?}", path))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")
            .context("configuring api-key db pragmas")?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.migrate()?;
        Ok(store)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory().context("opening in-memory api-key db")?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.migrate()?;
        Ok(store)
    }

    /// Idempotent schema migration. Safe to call repeatedly.
    pub fn migrate(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute_batch(SCHEMA_V1)
            .context("applying api-key schema v1")?;
        conn.execute(
            "INSERT OR REPLACE INTO api_keys_meta (key, value) VALUES (?1, ?2)",
            params![META_SCHEMA_VERSION, CURRENT_SCHEMA_VERSION.to_string()],
        )
        .context("writing api-key schema version")?;
        Ok(())
    }

    fn mint(&self, label: &str) -> Result<MintedKey> {
        let raw = self.generate_secret()?;
        let hash = hash_secret(&raw);
        let fingerprint = fingerprint_of(&hash);
        let label = bounded_label(label);
        let created_ms = now_ms();

        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT INTO api_keys (label, fingerprint, hash, created_ms, rotated_ms, revoked_ms)
             VALUES (?1, ?2, ?3, ?4, NULL, NULL)",
            params![label, fingerprint, hash, u64_to_i64(created_ms)],
        )
        .context("inserting api key")?;
        let id = conn.last_insert_rowid();

        Ok(MintedKey {
            record: ApiKeyRecord {
                id,
                label,
                fingerprint,
                created_ms,
                rotated_ms: None,
                revoked_ms: None,
            },
            raw_secret: raw,
        })
    }

    fn list(&self) -> Result<Vec<ApiKeyRecord>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, label, fingerprint, created_ms, rotated_ms, revoked_ms
             FROM api_keys ORDER BY id ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            let created: i64 = row.get(3)?;
            let rotated: Option<i64> = row.get(4)?;
            let revoked: Option<i64> = row.get(5)?;
            Ok(ApiKeyRecord {
                id: row.get(0)?,
                label: row.get(1)?,
                fingerprint: row.get(2)?,
                created_ms: i64_to_u64(created),
                rotated_ms: rotated.map(i64_to_u64),
                revoked_ms: revoked.map(i64_to_u64),
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    fn rotate(&self, id: i64) -> Result<MintedKey> {
        let raw = self.generate_secret()?;
        let hash = hash_secret(&raw);
        let fingerprint = fingerprint_of(&hash);
        let rotated_ms = now_ms();

        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let affected = conn
            .execute(
                "UPDATE api_keys SET hash = ?1, fingerprint = ?2, rotated_ms = ?3, revoked_ms = NULL
                 WHERE id = ?4",
                params![hash, fingerprint, u64_to_i64(rotated_ms), id],
            )
            .context("rotating api key")?;
        if affected == 0 {
            anyhow::bail!("no api key with id {id} to rotate");
        }

        // Read back the (unchanged) created_ms / label for the returned record.
        let (label, created_ms): (String, i64) = conn.query_row(
            "SELECT label, created_ms FROM api_keys WHERE id = ?1",
            params![id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;

        Ok(MintedKey {
            record: ApiKeyRecord {
                id,
                label,
                fingerprint,
                created_ms: i64_to_u64(created_ms),
                rotated_ms: Some(rotated_ms),
                revoked_ms: None,
            },
            raw_secret: raw,
        })
    }

    fn revoke(&self, id: i64) -> Result<()> {
        let revoked_ms = now_ms();
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        // Only stamp revoked_ms if not already revoked (preserve the first
        // revocation time); the WHERE still matches the row so a missing id is
        // detected separately below.
        let exists: bool = conn
            .query_row("SELECT 1 FROM api_keys WHERE id = ?1", params![id], |_| {
                Ok(true)
            })
            .optional()?
            .unwrap_or(false);
        if !exists {
            anyhow::bail!("no api key with id {id} to revoke");
        }
        conn.execute(
            "UPDATE api_keys SET revoked_ms = ?1 WHERE id = ?2 AND revoked_ms IS NULL",
            params![u64_to_i64(revoked_ms), id],
        )
        .context("revoking api key")?;
        Ok(())
    }

    fn verify(&self, presented: &str) -> bool {
        if presented.is_empty() {
            return false;
        }
        let presented_hash = hash_secret(presented);
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = match conn.prepare("SELECT hash FROM api_keys WHERE revoked_ms IS NULL") {
            Ok(stmt) => stmt,
            Err(err) => {
                tracing::warn!(error = %err, "api-key verify query failed; rejecting");
                return false;
            }
        };
        let hashes = stmt.query_map([], |row| row.get::<_, String>(0));
        let Ok(hashes) = hashes else {
            return false;
        };
        // Iterate every active hash and fold a constant-time compare so a match
        // anywhere returns true without short-circuiting on the first byte of a
        // candidate. (The set of active keys is small; this is not a hot path.)
        let mut matched = false;
        for stored in hashes.flatten() {
            if constant_time_eq(stored.as_bytes(), presented_hash.as_bytes()) {
                matched = true;
            }
        }
        matched
    }

    /// Source 32 bytes of OS entropy via the bundled SQLite `randomblob` (no new
    /// crate; see research.md D4) and render as `sf_<64 hex>`.
    fn generate_secret(&self) -> Result<String> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let bytes: Vec<u8> = conn
            .query_row("SELECT randomblob(32)", [], |row| row.get(0))
            .context("generating random api-key secret")?;
        let mut hex = String::with_capacity(2 + bytes.len() * 2);
        hex.push_str("sf_");
        use std::fmt::Write;
        for b in bytes {
            let _ = write!(hex, "{b:02x}");
        }
        Ok(hex)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// SHA-256 hex digest of a raw secret (the persisted form).
fn hash_secret(raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(64);
    use std::fmt::Write;
    for b in digest {
        let _ = write!(hex, "{b:02x}");
    }
    hex
}

/// A short, display-safe fingerprint derived from the hash (first 12 hex chars).
/// Non-reversible (it is a prefix of an already one-way hash) and never the raw.
fn fingerprint_of(hash: &str) -> String {
    hash.chars().take(12).collect()
}

fn bounded_label(raw: &str) -> String {
    let normalized: String = raw
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect();
    let trimmed = normalized.trim();
    let trimmed = if trimmed.is_empty() {
        "unnamed"
    } else {
        trimmed
    };
    if trimmed.len() <= MAX_LABEL_BYTES {
        return trimmed.to_string();
    }
    trimmed.chars().take(MAX_LABEL_BYTES).collect()
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn u64_to_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn i64_to_u64(value: i64) -> u64 {
    u64::try_from(value).unwrap_or_default()
}

/// Constant-time byte-slice equality (same discipline as `auth::constant_time_eq`).
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    let len_xor = a.len() ^ b.len();
    let mut diff: u8 = 0;
    for shift in (0..usize::BITS).step_by(8) {
        diff |= (len_xor >> shift) as u8;
    }
    let n = a.len().max(b.len());
    for i in 0..n {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_in_memory_is_enabled() {
        let store = ApiKeyStore::open_in_memory().expect("in-memory store");
        assert!(store.is_enabled());
        assert!(store.list().expect("list").is_empty());
    }

    #[test]
    fn mint_returns_raw_once_and_stores_hash_only() {
        let store = ApiKeyStore::open_in_memory().expect("store");
        let minted = store.mint("ci-runner").expect("mint");
        // Raw secret is present at mint and looks like a bearer token.
        assert!(minted.raw_secret.starts_with("sf_"));
        assert_eq!(minted.raw_secret.len(), 3 + 64, "sf_ + 32 bytes hex");
        assert_eq!(minted.record.label, "ci-runner");
        assert!(minted.record.is_active());

        // List never returns the raw secret (the record has no such field) and
        // the fingerprint is not the raw secret.
        let listed = store.list().expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].label, "ci-runner");
        assert_ne!(listed[0].fingerprint, minted.raw_secret);
        // The fingerprint is derived from the HASH, not the raw secret, so the
        // raw secret never contains the fingerprint as a substring.
        assert!(!minted.raw_secret.contains(&listed[0].fingerprint));
    }

    #[test]
    fn minted_key_verifies_revoked_key_does_not() {
        let store = ApiKeyStore::open_in_memory().expect("store");
        let minted = store.mint("agent-a").expect("mint");
        assert!(store.verify(&minted.raw_secret), "fresh key must verify");
        assert!(!store.verify("sf_wrong"), "wrong key must not verify");

        store.revoke(minted.record.id).expect("revoke");
        assert!(
            !store.verify(&minted.raw_secret),
            "revoked key must not verify"
        );
    }

    #[test]
    fn rotate_changes_secret_old_secret_stops_working() {
        let store = ApiKeyStore::open_in_memory().expect("store");
        let minted = store.mint("rotate-me").expect("mint");
        let old_raw = minted.raw_secret.clone();
        assert!(store.verify(&old_raw));

        let rotated = store.rotate(minted.record.id).expect("rotate");
        assert_ne!(rotated.raw_secret, old_raw, "rotation yields a new secret");
        assert!(!store.verify(&old_raw), "old secret stops working");
        assert!(store.verify(&rotated.raw_secret), "new secret works");
        assert!(rotated.record.rotated_ms.is_some());
    }

    #[test]
    fn revoke_is_idempotent_and_unknown_id_errors() {
        let store = ApiKeyStore::open_in_memory().expect("store");
        let minted = store.mint("k").expect("mint");
        store.revoke(minted.record.id).expect("first revoke");
        store
            .revoke(minted.record.id)
            .expect("second revoke is idempotent no-op");
        assert!(store.revoke(9999).is_err(), "unknown id must error");
    }

    #[test]
    fn disabled_store_degrades_safely() {
        let store = ApiKeyStore::Disabled;
        assert!(!store.is_enabled());
        assert!(store.list().expect("list").is_empty());
        assert!(!store.verify("anything"));
        assert!(store.mint("x").is_err());
        assert!(store.rotate(1).is_err());
        assert!(store.revoke(1).is_err());
    }

    #[test]
    fn persist_to_file_and_reopen_preserves_keys() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let raw = {
            let store = ApiKeyStore::open(tmp.path());
            let minted = store.mint("persist").expect("mint");
            assert!(store.verify(&minted.raw_secret));
            minted.raw_secret
        };
        // Reopen: the minted key must still verify (hash persisted).
        let store2 = ApiKeyStore::open(tmp.path());
        assert!(store2.verify(&raw), "reopened store verifies persisted key");
        assert_eq!(store2.list().expect("list").len(), 1);
    }

    /// D7 regression: `ApiKeyStore::open` takes the project ROOT (the production
    /// convention) and lands the db at the SINGLE-prefixed
    /// `root/.symforge/api-keys.db`. Before the fix, `serve.rs` passed the already
    /// `.symforge` data dir AND `open` joined a `.symforge/`-prefixed const,
    /// doubling the prefix to `root/.symforge/.symforge/api-keys.db` (shipped in
    /// 8.5.0). Assert the single path exists after a mint and the doubled path does
    /// NOT — the on-disk check the old test skipped by never inspecting the file.
    #[test]
    fn open_writes_single_prefixed_db_path_not_doubled() {
        use crate::paths::{API_KEYS_DB_NAME, SYMFORGE_DIR_NAME};
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();

        let store = ApiKeyStore::open(root);
        store.mint("d7").expect("mint writes the db file");

        let single = root.join(SYMFORGE_DIR_NAME).join(API_KEYS_DB_NAME);
        let doubled = root
            .join(SYMFORGE_DIR_NAME)
            .join(SYMFORGE_DIR_NAME)
            .join(API_KEYS_DB_NAME);
        assert!(
            single.is_file(),
            "api-keys db must be written to the single-prefixed {}",
            single.display()
        );
        assert!(
            !doubled.exists(),
            "api-keys db must NOT be written to the doubled {}",
            doubled.display()
        );
    }

    #[test]
    fn hashes_are_distinct_per_mint() {
        let store = ApiKeyStore::open_in_memory().expect("store");
        let a = store.mint("a").expect("mint a");
        let b = store.mint("b").expect("mint b");
        assert_ne!(a.raw_secret, b.raw_secret, "secrets are unique");
        assert_ne!(
            a.record.fingerprint, b.record.fingerprint,
            "fingerprints differ"
        );
    }

    #[test]
    fn empty_presented_secret_never_verifies() {
        let store = ApiKeyStore::open_in_memory().expect("store");
        store.mint("k").expect("mint");
        assert!(!store.verify(""), "empty secret must never verify");
    }
}
