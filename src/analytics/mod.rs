//! Local SQLite analytics store, schema, and bounded background writer queue.

mod queue;
pub mod schema;
pub mod store;

pub use queue::{
    AnalyticsEnqueueOutcome, AnalyticsQueueStatus, AnalyticsRecorder, AnalyticsWriter,
    DEFAULT_ANALYTICS_QUEUE_CAPACITY, MAX_ANALYTICS_QUEUE_CAPACITY,
    MAX_ANALYTICS_QUEUE_ERROR_BYTES,
};
pub use schema::{
    MAX_CAPABILITY_STATE_JSON_BYTES, MAX_CONFIGURED_SCOPE_BYTES, MAX_SURFACE_BYTES,
    MAX_TOOL_NAME_BYTES, SCHEMA_OUTCOME_CLASSES,
};
pub use store::{
    AnalyticsConfig, AnalyticsMode, AnalyticsObservation, AnalyticsScope, AnalyticsStatus,
    AnalyticsStore, AnalyticsSummary, AnalyticsSummaryCount, AnalyticsSurface,
    AnalyticsWriteOutcome, DEFAULT_ANALYTICS_EXPORT_LIMIT, DEFAULT_ANALYTICS_RETENTION_RECORDS,
    MAX_ANALYTICS_EXPORT_LIMIT, SqliteAnalyticsStore, StoredAnalyticsRecord,
};
