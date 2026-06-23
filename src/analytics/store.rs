use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use serde_json::json;

use crate::capability::CapabilityEvidence;
use crate::protocol::result_status::OutcomeClass;

use super::schema::{CURRENT_SCHEMA_VERSION, META_SCHEMA_VERSION, SCHEMA_V1};

pub const MAX_TOOL_NAME_BYTES: usize = 128;
pub const MAX_SURFACE_BYTES: usize = 64;
pub const MAX_CONFIGURED_SCOPE_BYTES: usize = 128;
pub const MAX_CAPABILITY_DETAIL_BYTES: usize = 256;
pub const MAX_CAPABILITY_STATE_JSON_BYTES: usize = 4096;
pub const MAX_CAPABILITY_ITEMS: usize = 16;
pub const DEFAULT_ANALYTICS_EXPORT_LIMIT: usize = 100;
pub const MAX_ANALYTICS_EXPORT_LIMIT: usize = 1_000;
pub const DEFAULT_ANALYTICS_RETENTION_RECORDS: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalyticsMode {
    Disabled,
    Enabled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyticsConfig {
    pub mode: AnalyticsMode,
    pub db_path: PathBuf,
}

impl AnalyticsConfig {
    pub fn disabled(db_path: impl Into<PathBuf>) -> Self {
        Self {
            mode: AnalyticsMode::Disabled,
            db_path: db_path.into(),
        }
    }

    pub fn enabled(db_path: impl Into<PathBuf>) -> Self {
        Self {
            mode: AnalyticsMode::Enabled,
            db_path: db_path.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnalyticsStatus {
    Disabled {
        db_path: PathBuf,
    },
    Enabled {
        db_path: PathBuf,
        schema_version: u32,
    },
}

impl AnalyticsStatus {
    pub const fn is_disabled(&self) -> bool {
        matches!(self, Self::Disabled { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnalyticsWriteOutcome {
    Disabled(AnalyticsStatus),
    Recorded { id: i64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnalyticsSurface {
    Tool,
    Resource,
    Prompt,
    Hook,
    Sidecar,
    Other(String),
}

impl AnalyticsSurface {
    fn storage_value(&self) -> String {
        match self {
            Self::Tool => "tool".to_string(),
            Self::Resource => "resource".to_string(),
            Self::Prompt => "prompt".to_string(),
            Self::Hook => "hook".to_string(),
            Self::Sidecar => "sidecar".to_string(),
            Self::Other(value) => bounded_text(value, MAX_SURFACE_BYTES),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnalyticsScope {
    Disabled,
    Session,
    Workspace,
    Persistent,
    Other(String),
}

impl AnalyticsScope {
    fn storage_value(&self) -> String {
        match self {
            Self::Disabled => "disabled".to_string(),
            Self::Session => "session".to_string(),
            Self::Workspace => "workspace".to_string(),
            Self::Persistent => "persistent".to_string(),
            Self::Other(value) => bounded_text(value, MAX_CONFIGURED_SCOPE_BYTES),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyticsObservation {
    pub tool_name: String,
    pub surface: AnalyticsSurface,
    pub configured_scope: AnalyticsScope,
    pub response_bytes: u64,
    pub estimated_tokens: Option<u64>,
    pub duration: Duration,
    pub success: bool,
    pub outcome_class: OutcomeClass,
    pub capability_state: Vec<CapabilityEvidence>,
}

impl AnalyticsObservation {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tool_name: impl Into<String>,
        surface: AnalyticsSurface,
        configured_scope: AnalyticsScope,
        response_bytes: u64,
        estimated_tokens: Option<u64>,
        duration: Duration,
        success: bool,
        outcome_class: OutcomeClass,
    ) -> Self {
        Self {
            tool_name: tool_name.into(),
            surface,
            configured_scope,
            response_bytes,
            estimated_tokens,
            duration,
            success,
            outcome_class,
            capability_state: Vec::new(),
        }
    }

    pub fn with_capability_state(mut self, capability_state: Vec<CapabilityEvidence>) -> Self {
        self.capability_state = capability_state;
        self
    }

    pub fn bounded_for_queue(&self) -> Self {
        let capability_state = self
            .capability_state
            .iter()
            .take(MAX_CAPABILITY_ITEMS)
            .cloned()
            .map(|mut evidence| {
                if let Some(detail) = evidence.detail.as_deref() {
                    evidence.detail = Some(bounded_text(detail, MAX_CAPABILITY_DETAIL_BYTES));
                }
                evidence
            })
            .collect();

        Self {
            tool_name: bounded_text(&self.tool_name, MAX_TOOL_NAME_BYTES),
            surface: bounded_surface(&self.surface),
            configured_scope: bounded_scope(&self.configured_scope),
            response_bytes: self.response_bytes,
            estimated_tokens: self.estimated_tokens,
            duration: self.duration,
            success: self.success,
            outcome_class: self.outcome_class,
            capability_state,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredAnalyticsRecord {
    pub id: i64,
    pub tool_name: String,
    pub surface: String,
    pub configured_scope: String,
    pub response_bytes: u64,
    pub estimated_tokens: Option<u64>,
    pub duration_ms: u64,
    pub success: bool,
    pub outcome_class: String,
    pub capability_state_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AnalyticsSummary {
    pub total_records: u64,
    pub success_count: u64,
    pub failure_count: u64,
    pub latest_id: Option<i64>,
    pub tool_counts: Vec<AnalyticsSummaryCount>,
    pub outcome_counts: Vec<AnalyticsSummaryCount>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AnalyticsSummaryCount {
    pub value: String,
    pub count: u64,
}

#[derive(Debug, Clone)]
pub enum AnalyticsStore {
    Disabled { db_path: PathBuf },
    Enabled(SqliteAnalyticsStore),
}

impl AnalyticsStore {
    pub fn open(config: AnalyticsConfig) -> Result<Self> {
        match config.mode {
            AnalyticsMode::Disabled => Ok(Self::disabled(config.db_path)),
            AnalyticsMode::Enabled => {
                Ok(Self::Enabled(SqliteAnalyticsStore::open(&config.db_path)?))
            }
        }
    }

    pub fn disabled(db_path: impl Into<PathBuf>) -> Self {
        Self::Disabled {
            db_path: db_path.into(),
        }
    }

    pub fn status(&self) -> Result<AnalyticsStatus> {
        match self {
            Self::Disabled { db_path } => Ok(AnalyticsStatus::Disabled {
                db_path: db_path.clone(),
            }),
            Self::Enabled(store) => store.status(),
        }
    }

    pub fn record(&self, observation: &AnalyticsObservation) -> Result<AnalyticsWriteOutcome> {
        match self {
            Self::Disabled { db_path } => {
                Ok(AnalyticsWriteOutcome::Disabled(AnalyticsStatus::Disabled {
                    db_path: db_path.clone(),
                }))
            }
            Self::Enabled(store) => Ok(AnalyticsWriteOutcome::Recorded {
                id: store.record(observation)?,
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SqliteAnalyticsStore {
    db_path: PathBuf,
    conn: Arc<Mutex<Connection>>,
}

impl SqliteAnalyticsStore {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating analytics db parent dir {:?}", parent))?;
        }
        let conn = Connection::open(path)
            .with_context(|| format!("opening analytics db at {:?}", path))?;
        let store = Self {
            db_path: path.to_path_buf(),
            conn: Arc::new(Mutex::new(conn)),
        };
        store.migrate()?;
        Ok(store)
    }

    pub fn open_in_memory() -> Result<Self> {
        let store = Self {
            db_path: PathBuf::from(":memory:"),
            conn: Arc::new(Mutex::new(Connection::open_in_memory()?)),
        };
        store.migrate()?;
        Ok(store)
    }

    pub fn status(&self) -> Result<AnalyticsStatus> {
        Ok(AnalyticsStatus::Enabled {
            db_path: self.db_path.clone(),
            schema_version: self.schema_version()?,
        })
    }

    pub fn migrate(&self) -> Result<()> {
        let conn = self.conn.lock().expect("analytics mutex poisoned");
        conn.execute_batch(SCHEMA_V1)
            .context("applying analytics schema v1")?;
        conn.execute(
            "INSERT OR REPLACE INTO analytics_meta (key, value) VALUES (?1, ?2)",
            params![META_SCHEMA_VERSION, CURRENT_SCHEMA_VERSION.to_string()],
        )?;
        Ok(())
    }

    pub fn schema_version(&self) -> Result<u32> {
        let conn = self.conn.lock().expect("analytics mutex poisoned");
        let value: Option<String> = conn
            .query_row(
                "SELECT value FROM analytics_meta WHERE key = ?1",
                params![META_SCHEMA_VERSION],
                |row| row.get(0),
            )
            .optional()?;
        Ok(value.and_then(|v| v.parse().ok()).unwrap_or(0))
    }

    pub fn record(&self, observation: &AnalyticsObservation) -> Result<i64> {
        let sanitized = SanitizedObservation::from(observation);
        let conn = self.conn.lock().expect("analytics mutex poisoned");
        conn.execute(
            "INSERT INTO analytics_tool_calls (
                tool_name,
                surface,
                configured_scope,
                response_bytes,
                estimated_tokens,
                duration_ms,
                success,
                outcome_class,
                capability_state_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                sanitized.tool_name,
                sanitized.surface,
                sanitized.configured_scope,
                sanitized.response_bytes,
                sanitized.estimated_tokens,
                sanitized.duration_ms,
                sanitized.success,
                sanitized.outcome_class,
                sanitized.capability_state_json,
            ],
        )?;
        let id = conn.last_insert_rowid();
        enforce_record_retention(&conn, DEFAULT_ANALYTICS_RETENTION_RECORDS)?;
        Ok(id)
    }

    pub fn recent_records(&self, limit: usize) -> Result<Vec<StoredAnalyticsRecord>> {
        let conn = self.conn.lock().expect("analytics mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT
                id,
                tool_name,
                surface,
                configured_scope,
                response_bytes,
                estimated_tokens,
                duration_ms,
                success,
                outcome_class,
                capability_state_json
            FROM analytics_tool_calls
            ORDER BY id DESC
            LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![usize_to_i64(limit)], |row| {
            let response_bytes: i64 = row.get(4)?;
            let estimated_tokens: Option<i64> = row.get(5)?;
            let duration_ms: i64 = row.get(6)?;
            let success: i64 = row.get(7)?;
            Ok(StoredAnalyticsRecord {
                id: row.get(0)?,
                tool_name: row.get(1)?,
                surface: row.get(2)?,
                configured_scope: row.get(3)?,
                response_bytes: i64_to_u64(response_bytes),
                estimated_tokens: estimated_tokens.map(i64_to_u64),
                duration_ms: i64_to_u64(duration_ms),
                success: success != 0,
                outcome_class: row.get(8)?,
                capability_state_json: row.get(9)?,
            })
        })?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn export_records(&self, limit: usize) -> Result<Vec<StoredAnalyticsRecord>> {
        self.recent_records(normalize_export_limit(limit))
    }

    pub fn summary(&self, group_limit: usize) -> Result<AnalyticsSummary> {
        let conn = self.conn.lock().expect("analytics mutex poisoned");
        let total_records = query_count(&conn, "SELECT COUNT(*) FROM analytics_tool_calls")?;
        let success_count = query_count(
            &conn,
            "SELECT COUNT(*) FROM analytics_tool_calls WHERE success = 1",
        )?;
        let failure_count = query_count(
            &conn,
            "SELECT COUNT(*) FROM analytics_tool_calls WHERE success = 0",
        )?;
        let latest_id = conn.query_row("SELECT MAX(id) FROM analytics_tool_calls", [], |row| {
            row.get::<_, Option<i64>>(0)
        })?;

        Ok(AnalyticsSummary {
            total_records,
            success_count,
            failure_count,
            latest_id,
            tool_counts: grouped_counts(&conn, "tool_name", group_limit)?,
            outcome_counts: grouped_counts(&conn, "outcome_class", group_limit)?,
        })
    }

    pub fn enforce_retention(&self, max_records: usize) -> Result<usize> {
        let conn = self.conn.lock().expect("analytics mutex poisoned");
        enforce_record_retention(&conn, max_records)
    }
}

fn normalize_export_limit(limit: usize) -> usize {
    limit.min(MAX_ANALYTICS_EXPORT_LIMIT)
}

fn query_count(conn: &Connection, sql: &str) -> Result<u64> {
    let count: i64 = conn.query_row(sql, [], |row| row.get(0))?;
    Ok(i64_to_u64(count))
}

fn grouped_counts(
    conn: &Connection,
    column: &'static str,
    limit: usize,
) -> Result<Vec<AnalyticsSummaryCount>> {
    let sql = format!(
        "SELECT {column}, COUNT(*) FROM analytics_tool_calls \
         GROUP BY {column} \
         ORDER BY COUNT(*) DESC, {column} ASC \
         LIMIT ?1"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![usize_to_i64(limit)], |row| {
        let count: i64 = row.get(1)?;
        Ok(AnalyticsSummaryCount {
            value: row.get(0)?,
            count: i64_to_u64(count),
        })
    })?;

    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

fn enforce_record_retention(conn: &Connection, max_records: usize) -> Result<usize> {
    conn.execute(
        "DELETE FROM analytics_tool_calls
         WHERE id NOT IN (
             SELECT id FROM analytics_tool_calls
             ORDER BY id DESC
             LIMIT ?1
         )",
        params![usize_to_i64(max_records)],
    )
    .map_err(Into::into)
}

struct SanitizedObservation {
    tool_name: String,
    surface: String,
    configured_scope: String,
    response_bytes: i64,
    estimated_tokens: Option<i64>,
    duration_ms: i64,
    success: i64,
    outcome_class: &'static str,
    capability_state_json: String,
}

impl From<&AnalyticsObservation> for SanitizedObservation {
    fn from(observation: &AnalyticsObservation) -> Self {
        Self {
            tool_name: bounded_text(&observation.tool_name, MAX_TOOL_NAME_BYTES),
            surface: observation.surface.storage_value(),
            configured_scope: observation.configured_scope.storage_value(),
            response_bytes: u64_to_i64(observation.response_bytes),
            estimated_tokens: observation.estimated_tokens.map(u64_to_i64),
            duration_ms: u128_to_i64(observation.duration.as_millis()),
            success: i64::from(observation.success),
            outcome_class: observation.outcome_class.as_str(),
            capability_state_json: capability_state_json(&observation.capability_state),
        }
    }
}

fn capability_state_json(capability_state: &[CapabilityEvidence]) -> String {
    let mut values = capability_state
        .iter()
        .take(MAX_CAPABILITY_ITEMS)
        .map(|evidence| {
            json!({
                "capability": evidence.capability.to_string(),
                "status": evidence.status.to_string(),
                "freshness": evidence.freshness.to_string(),
                "cost": evidence.cost.to_string(),
                "safety": evidence.safety.to_string(),
                "detail": evidence
                    .detail
                    .as_deref()
                    .map(|detail| bounded_text(detail, MAX_CAPABILITY_DETAIL_BYTES)),
            })
        })
        .collect::<Vec<_>>();

    loop {
        let value = serde_json::to_string(&values).expect("analytics capability JSON serializes");
        if value.len() <= MAX_CAPABILITY_STATE_JSON_BYTES || values.is_empty() {
            return value;
        }
        values.pop();
    }
}

pub(crate) fn bounded_text(raw: &str, max_bytes: usize) -> String {
    if contains_sensitive_marker(raw) {
        return "[redacted]".to_string();
    }

    let normalized = raw
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect::<String>();

    truncate_utf8(&normalized, max_bytes)
}

fn bounded_surface(surface: &AnalyticsSurface) -> AnalyticsSurface {
    match surface {
        AnalyticsSurface::Other(value) => {
            AnalyticsSurface::Other(bounded_text(value, MAX_SURFACE_BYTES))
        }
        known => known.clone(),
    }
}

fn bounded_scope(scope: &AnalyticsScope) -> AnalyticsScope {
    match scope {
        AnalyticsScope::Other(value) => {
            AnalyticsScope::Other(bounded_text(value, MAX_CONFIGURED_SCOPE_BYTES))
        }
        known => known.clone(),
    }
}

fn contains_sensitive_marker(raw: &str) -> bool {
    let lower = raw.to_ascii_lowercase();
    [
        "authorization",
        "bearer ",
        "api_key",
        "apikey",
        "access_token",
        "refresh_token",
        "password",
        "private key",
        "secret",
        "sk-",
        "ghp_",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

fn truncate_utf8(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }

    let mut out = String::new();
    let budget = max_bytes.saturating_sub(3);
    for ch in value.chars() {
        if out.len() + ch.len_utf8() > budget {
            break;
        }
        out.push(ch);
    }
    out.push_str("...");
    out
}

fn u64_to_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn u128_to_i64(value: u128) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn usize_to_i64(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn i64_to_u64(value: i64) -> u64 {
    u64::try_from(value).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::capability::{
        CapabilityCost, CapabilityEvidence, CapabilityFreshness, CapabilityName, CapabilitySafety,
        CapabilityStatus,
    };

    fn sample_observation() -> AnalyticsObservation {
        AnalyticsObservation::new(
            "search_text",
            AnalyticsSurface::Tool,
            AnalyticsScope::Persistent,
            2048,
            Some(512),
            Duration::from_millis(37),
            true,
            OutcomeClass::Found,
        )
        .with_capability_state(vec![
            CapabilityEvidence::new(CapabilityName::FrecencyRanking, CapabilityStatus::Ready)
                .with_freshness(CapabilityFreshness::Current)
                .with_cost(CapabilityCost::Low)
                .with_safety(CapabilitySafety::ReadOnly)
                .with_detail("persistent history available"),
        ])
    }

    #[test]
    fn open_in_memory_creates_schema_at_current_version() {
        let store = SqliteAnalyticsStore::open_in_memory().expect("analytics store");

        assert_eq!(
            store.schema_version().expect("schema version"),
            CURRENT_SCHEMA_VERSION
        );
    }

    #[test]
    fn migration_is_idempotent_and_preserves_current_version() {
        let store = SqliteAnalyticsStore::open_in_memory().expect("analytics store");

        store.migrate().expect("first migrate");
        store.migrate().expect("second migrate");

        assert_eq!(
            store.schema_version().expect("schema version"),
            CURRENT_SCHEMA_VERSION
        );
    }

    #[test]
    fn disabled_store_reports_status_and_keeps_filesystem_footprint_free() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let db_path = crate::paths::symforge_db_path(tmp.path(), crate::paths::ANALYTICS_DB_NAME);
        let store = AnalyticsStore::open(AnalyticsConfig::disabled(&db_path)).expect("disabled");

        assert!(store.status().expect("status").is_disabled());
        assert!(
            !db_path.exists(),
            "disabled analytics must not create the database"
        );
        assert!(
            !db_path.parent().unwrap().exists(),
            "disabled analytics must not create the .symforge directory"
        );

        let outcome = store
            .record(&sample_observation())
            .expect("disabled record outcome");
        assert!(matches!(
            outcome,
            AnalyticsWriteOutcome::Disabled(AnalyticsStatus::Disabled { .. })
        ));
        assert!(
            !db_path.exists(),
            "disabled analytics record path must remain no-op"
        );
        assert!(
            !db_path.parent().unwrap().exists(),
            "disabled analytics record path must remain no-footprint"
        );
    }

    #[test]
    fn enabled_store_creates_db_and_records_bounded_local_metadata() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let db_path = crate::paths::symforge_db_path(tmp.path(), crate::paths::ANALYTICS_DB_NAME);
        let store = SqliteAnalyticsStore::open(&db_path).expect("analytics store");

        assert!(db_path.exists(), "enabled analytics creates the database");
        let id = store.record(&sample_observation()).expect("record");
        let records = store.recent_records(10).expect("records");

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].id, id);
        assert_eq!(records[0].tool_name, "search_text");
        assert_eq!(records[0].surface, "tool");
        assert_eq!(records[0].configured_scope, "persistent");
        assert_eq!(records[0].response_bytes, 2048);
        assert_eq!(records[0].estimated_tokens, Some(512));
        assert_eq!(records[0].duration_ms, 37);
        assert!(records[0].success);
        assert_eq!(records[0].outcome_class, "found");
        assert!(
            records[0]
                .capability_state_json
                .contains("frecency ranking")
        );
    }

    #[test]
    fn retention_deletes_older_records_and_keeps_recent_records() {
        let store = SqliteAnalyticsStore::open_in_memory().expect("analytics store");
        for index in 0..5 {
            let mut observation = sample_observation();
            observation.tool_name = format!("tool_{index}");
            store.record(&observation).expect("record");
        }

        let deleted = store.enforce_retention(2).expect("retention");
        let records = store.recent_records(10).expect("records");

        assert_eq!(deleted, 3);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].tool_name, "tool_4");
        assert_eq!(records[1].tool_name, "tool_3");
    }

    #[test]
    fn oversized_and_sensitive_metadata_is_not_stored_raw() {
        let store = SqliteAnalyticsStore::open_in_memory().expect("analytics store");
        let long_tool_name = format!("get_file_content_{}", "x".repeat(500));
        let observation = AnalyticsObservation::new(
            long_tool_name,
            AnalyticsSurface::Other("Authorization: Bearer sk-test-secret".to_string()),
            AnalyticsScope::Other("password=secret-value".to_string()),
            u64::MAX,
            Some(u64::MAX),
            Duration::from_millis(u64::MAX),
            false,
            OutcomeClass::InternalFailure,
        )
        .with_capability_state(vec![
            CapabilityEvidence::new(
                CapabilityName::RankingDiagnostics,
                CapabilityStatus::DisabledByPolicy,
            )
            .with_detail(format!("api_key=sk-secret {}", "y".repeat(1000))),
        ]);

        store.record(&observation).expect("record");
        let record = store
            .recent_records(1)
            .expect("records")
            .pop()
            .expect("one record");
        let stored = format!(
            "{} {} {} {}",
            record.tool_name, record.surface, record.configured_scope, record.capability_state_json
        );

        assert!(record.tool_name.len() <= MAX_TOOL_NAME_BYTES);
        assert_eq!(record.surface, "[redacted]");
        assert_eq!(record.configured_scope, "[redacted]");
        assert!(record.capability_state_json.len() <= MAX_CAPABILITY_STATE_JSON_BYTES);
        assert!(!stored.contains("sk-secret"));
        assert!(!stored.contains("password=secret-value"));
        assert!(!stored.contains("api_key"));
        assert_eq!(record.response_bytes, i64::MAX as u64);
        assert_eq!(record.estimated_tokens, Some(i64::MAX as u64));
        assert_eq!(record.duration_ms, i64::MAX as u64);
        assert_eq!(record.outcome_class, "internal_failure");
    }
}
