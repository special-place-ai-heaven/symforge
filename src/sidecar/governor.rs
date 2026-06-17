//! Request governor — micro-queuing engine for tool dispatch.
//!
//! Every request entering the server gets a unique `RequestId`, timestamp,
//! and is tracked through its lifecycle: queued → executing → completed.
//!
//! The governor prevents collisions between concurrent requests:
//! - Weighted semaphore limits total concurrency (default 16 permits)
//! - Write operations (edits, renames) are serialized via a write gate
//! - Read operations run concurrently but never overlap with writes
//! - Per-request timeouts at both queue and execution phases
//! - Full observability: in-flight requests visible in health output

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::Mutex;
use std::time::{Duration, Instant};

use tokio::sync::{RwLock, Semaphore};

/// Default maximum concurrent tool calls (total permits).
/// Sized for multi-agent workloads: 4 agents × 4 concurrent calls = 16 permits.
const DEFAULT_MAX_CONCURRENCY: usize = 16;

/// Default per-request execution timeout.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Default maximum time a request can wait in queue for permits.
const DEFAULT_QUEUE_TIMEOUT: Duration = Duration::from_secs(15);

// ---------------------------------------------------------------------------
// Request identity and tracking
// ---------------------------------------------------------------------------

/// Unique identifier for a governed request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RequestId(pub u64);

impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "req-{}", self.0)
    }
}

/// Lifecycle phase of a tracked request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum RequestPhase {
    /// Waiting in queue for semaphore permits.
    Queued,
    /// Actively executing.
    Executing,
}

/// A request currently tracked by the governor.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TrackedRequest {
    pub id: u64,
    pub tool: String,
    pub weight: ToolWeight,
    pub phase: RequestPhase,
    /// Milliseconds since this request was submitted.
    pub age_ms: u64,
    /// Milliseconds spent in the current phase.
    pub phase_ms: u64,
}

/// Internal tracking entry (not serialized directly — converted to TrackedRequest).
struct RequestEntry {
    tool: String,
    weight: ToolWeight,
    phase: RequestPhase,
    submitted_at: Instant,
    phase_started_at: Instant,
}

// ---------------------------------------------------------------------------
// Tool weight classification
// ---------------------------------------------------------------------------

/// Tool weight — how many semaphore permits each operation costs,
/// and whether it requires the write gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum ToolWeight {
    /// Read-only queries: get_symbol, search_text, get_file_context, etc.
    /// Cost: 1 permit. No write gate.
    Light,
    /// Moderate operations: get_repo_map full, analyze_file_impact.
    /// Cost: 2 permits. No write gate.
    Medium,
    /// Heavy write operations: batch_edit, batch_rename, batch_insert, index_folder.
    /// Cost: 3 permits. Requires exclusive write gate.
    Heavy,
}

impl ToolWeight {
    /// Number of semaphore permits this weight consumes.
    pub fn permits(self) -> u32 {
        match self {
            ToolWeight::Light => 1,
            ToolWeight::Medium => 2,
            ToolWeight::Heavy => 3,
        }
    }

    /// Whether this weight class requires the exclusive write gate.
    pub fn needs_write_gate(self) -> bool {
        matches!(self, ToolWeight::Heavy)
    }
}

/// Classify a tool name into its weight category.
pub fn classify_tool(tool_name: &str) -> ToolWeight {
    match tool_name {
        // Heavy: write operations and full indexing — exclusive access
        "index_folder" | "batch_edit" | "batch_rename" | "batch_insert" => ToolWeight::Heavy,

        // Heavy: single-file write operations — must serialize with other writers
        "replace_symbol_body" | "edit_within_symbol" | "insert_symbol" | "delete_symbol" => {
            ToolWeight::Heavy
        }

        // Medium: operations that scan many files but only read
        "analyze_file_impact" | "sidecar/impact" => ToolWeight::Medium,

        // Light: sidecar endpoints (reads, lookups, health)
        "sidecar/outline"
        | "sidecar/prompt-context"
        | "sidecar/repo-map"
        | "sidecar/symbol-context"
        | "sidecar/health"
        | "sidecar/stats" => ToolWeight::Light,

        // Light: everything else (reads, searches, lookups)
        _ => ToolWeight::Light,
    }
}

// ---------------------------------------------------------------------------
// Governor stats
// ---------------------------------------------------------------------------

/// Lifetime counters for the governor.
#[derive(Debug)]
pub struct GovernorStats {
    pub total_submitted: AtomicU64,
    pub total_completed: AtomicU64,
    pub total_timed_out: AtomicU64,
    pub total_queue_rejected: AtomicU64,
    pub peak_in_flight: AtomicU64,
}

impl GovernorStats {
    fn new() -> Self {
        Self {
            total_submitted: AtomicU64::new(0),
            total_completed: AtomicU64::new(0),
            total_timed_out: AtomicU64::new(0),
            total_queue_rejected: AtomicU64::new(0),
            peak_in_flight: AtomicU64::new(0),
        }
    }
}

/// Serializable snapshot of governor state and all in-flight requests.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GovernorSnapshot {
    pub max_concurrency: usize,
    pub available_permits: usize,
    pub in_flight: Vec<TrackedRequest>,
    pub total_submitted: u64,
    pub total_completed: u64,
    pub total_timed_out: u64,
    pub total_queue_rejected: u64,
    pub peak_in_flight: u64,
}

// ---------------------------------------------------------------------------
// RequestGovernor
// ---------------------------------------------------------------------------

/// Micro-queuing engine for tool dispatch.
///
/// Every request gets:
/// - A unique `RequestId` (monotonic counter)
/// - Timestamped lifecycle tracking (submitted → executing → done)
/// - Weighted permit acquisition from a bounded semaphore
/// - Exclusive write gate for heavy operations (prevents read/write collisions)
/// - Enforced timeouts at queue and execution phases
///
/// The governor is shared across all request paths (daemon + local mode).
#[derive(Clone)]
pub struct RequestGovernor {
    /// Bounded concurrency semaphore.
    semaphore: Arc<Semaphore>,
    /// Read-write gate: heavy ops take a write lock, reads take a read lock.
    /// This ensures batch_edit/batch_rename never run concurrently with reads.
    write_gate: Arc<RwLock<()>>,
    /// Execution timeout.
    timeout: Duration,
    /// Queue wait timeout.
    queue_timeout: Duration,
    /// Total permits available.
    max_concurrency: usize,
    /// Monotonic request ID counter.
    next_id: Arc<AtomicU64>,
    /// Currently tracked requests (queued + executing).
    active: Arc<Mutex<HashMap<u64, RequestEntry>>>,
    /// Lifetime stats.
    pub stats: Arc<GovernorStats>,
}

impl RequestGovernor {
    /// Create a governor with default settings.
    pub fn new() -> Self {
        Self::with_config(
            DEFAULT_MAX_CONCURRENCY,
            DEFAULT_TIMEOUT,
            DEFAULT_QUEUE_TIMEOUT,
        )
    }

    /// Create a governor with custom limits.
    pub fn with_config(max_concurrency: usize, timeout: Duration, queue_timeout: Duration) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(max_concurrency)),
            write_gate: Arc::new(RwLock::new(())),
            timeout,
            queue_timeout,
            max_concurrency,
            next_id: Arc::new(AtomicU64::new(1)),
            active: Arc::new(Mutex::new(HashMap::new())),
            stats: Arc::new(GovernorStats::new()),
        }
    }

    /// Maximum concurrency (total permits).
    pub fn max_concurrency(&self) -> usize {
        self.max_concurrency
    }

    /// Available permits right now.
    pub fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }

    /// Acquire one concurrency permit at a transport boundary (P2-F).
    ///
    /// The `/mcp` Streamable-HTTP route is served by rmcp's transport, not by
    /// [`Self::execute`] (which is tool-name-keyed and wraps a future). To bound
    /// concurrent operator clients without forking the rmcp dispatch, the HTTP
    /// layer acquires one owned permit here for the lifetime of each request and
    /// drops it when the request completes — a small acquire/release around the
    /// shared dispatch. Each `/mcp` request costs one permit (the governor's
    /// `Light` weight), so up to [`Self::max_concurrency`] requests run at once
    /// and the rest queue.
    ///
    /// Honors the same queue timeout as [`Self::execute`]: if no permit becomes
    /// available within `queue_timeout`, returns [`GovernorError::QueueTimeout`]
    /// so the caller can shed load (HTTP `503`) instead of blocking unboundedly.
    /// The returned [`tokio::sync::OwnedSemaphorePermit`] releases the permit on
    /// drop. Counts toward the governor's submitted/queue-rejected stats so the
    /// HTTP boundary is observable alongside tool dispatch.
    pub async fn acquire_request_slot(
        &self,
    ) -> Result<tokio::sync::OwnedSemaphorePermit, GovernorError> {
        self.stats.total_submitted.fetch_add(1, Ordering::Relaxed);
        match tokio::time::timeout(
            self.queue_timeout,
            Arc::clone(&self.semaphore).acquire_owned(),
        )
        .await
        {
            Ok(Ok(permit)) => Ok(permit),
            Ok(Err(_closed)) => {
                self.stats
                    .total_queue_rejected
                    .fetch_add(1, Ordering::Relaxed);
                Err(GovernorError::SemaphoreClosed)
            }
            Err(_elapsed) => {
                self.stats
                    .total_queue_rejected
                    .fetch_add(1, Ordering::Relaxed);
                Err(GovernorError::QueueTimeout {
                    request_id: RequestId(0),
                    tool: "/mcp".to_string(),
                    waited: self.queue_timeout,
                    weight: ToolWeight::Light,
                    in_flight: self.active_count(),
                })
            }
        }
    }

    /// Snapshot of governor state including all in-flight requests.
    pub fn snapshot(&self) -> GovernorSnapshot {
        let now = Instant::now();
        let active = self.active.lock();
        let in_flight: Vec<TrackedRequest> = active
            .iter()
            .map(|(&id, entry)| TrackedRequest {
                id,
                tool: entry.tool.clone(),
                weight: entry.weight,
                phase: entry.phase,
                age_ms: now.duration_since(entry.submitted_at).as_millis() as u64,
                phase_ms: now.duration_since(entry.phase_started_at).as_millis() as u64,
            })
            .collect();

        GovernorSnapshot {
            max_concurrency: self.max_concurrency,
            available_permits: self.semaphore.available_permits(),
            in_flight,
            total_submitted: self.stats.total_submitted.load(Ordering::Relaxed),
            total_completed: self.stats.total_completed.load(Ordering::Relaxed),
            total_timed_out: self.stats.total_timed_out.load(Ordering::Relaxed),
            total_queue_rejected: self.stats.total_queue_rejected.load(Ordering::Relaxed),
            peak_in_flight: self.stats.peak_in_flight.load(Ordering::Relaxed),
        }
    }

    /// Execute a tool call under governor control.
    ///
    /// Lifecycle:
    /// 1. Assign request ID, record as Queued
    /// 2. Acquire semaphore permits (weighted by tool class)
    /// 3. Acquire write gate (exclusive for heavy ops, shared for reads)
    /// 4. Transition to Executing, run with timeout
    /// 5. Clean up tracking, update stats
    pub async fn execute<F, T>(&self, tool_name: &str, fut: F) -> Result<T, GovernorError>
    where
        F: std::future::Future<Output = T>,
    {
        let weight = classify_tool(tool_name);
        let permits_needed = weight.permits();
        let req_id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let now = Instant::now();

        // Register the request as Queued.
        self.stats.total_submitted.fetch_add(1, Ordering::Relaxed);
        {
            let mut active = self.active.lock();
            active.insert(
                req_id,
                RequestEntry {
                    tool: tool_name.to_string(),
                    weight,
                    phase: RequestPhase::Queued,
                    submitted_at: now,
                    phase_started_at: now,
                },
            );
        }

        // Acquire semaphore permits with queue timeout.
        let permit = match tokio::time::timeout(
            self.queue_timeout,
            self.semaphore.acquire_many(permits_needed),
        )
        .await
        {
            Ok(Ok(permit)) => permit,
            Ok(Err(_closed)) => {
                self.remove_request(req_id);
                self.stats
                    .total_queue_rejected
                    .fetch_add(1, Ordering::Relaxed);
                return Err(GovernorError::SemaphoreClosed);
            }
            Err(_elapsed) => {
                self.remove_request(req_id);
                self.stats
                    .total_queue_rejected
                    .fetch_add(1, Ordering::Relaxed);
                return Err(GovernorError::QueueTimeout {
                    request_id: RequestId(req_id),
                    tool: tool_name.to_string(),
                    waited: self.queue_timeout,
                    weight,
                    in_flight: self.active_count(),
                });
            }
        };

        // Acquire write gate and execute.
        // Heavy ops take exclusive (write) access — blocks all other ops.
        // Light/Medium ops take shared (read) access — concurrent with each other.
        // The gate guard must be held in the same scope as execution to avoid
        // lifetime issues with the RwLockGuard.
        let result = if weight.needs_write_gate() {
            let gate = match tokio::time::timeout(self.queue_timeout, self.write_gate.write()).await
            {
                Ok(guard) => guard,
                Err(_elapsed) => {
                    self.remove_request(req_id);
                    drop(permit);
                    self.stats
                        .total_queue_rejected
                        .fetch_add(1, Ordering::Relaxed);
                    return Err(GovernorError::WriteGateTimeout {
                        request_id: RequestId(req_id),
                        tool: tool_name.to_string(),
                    });
                }
            };
            self.transition_to_executing(req_id);
            let r = tokio::time::timeout(self.timeout, fut).await;
            drop(gate);
            r
        } else {
            let gate = match tokio::time::timeout(self.queue_timeout, self.write_gate.read()).await
            {
                Ok(guard) => guard,
                Err(_elapsed) => {
                    self.remove_request(req_id);
                    drop(permit);
                    self.stats
                        .total_queue_rejected
                        .fetch_add(1, Ordering::Relaxed);
                    return Err(GovernorError::WriteGateTimeout {
                        request_id: RequestId(req_id),
                        tool: tool_name.to_string(),
                    });
                }
            };
            self.transition_to_executing(req_id);
            let r = tokio::time::timeout(self.timeout, fut).await;
            drop(gate);
            r
        };

        // Clean up.
        self.remove_request(req_id);
        drop(permit);

        match result {
            Ok(value) => {
                self.stats.total_completed.fetch_add(1, Ordering::Relaxed);
                Ok(value)
            }
            Err(_elapsed) => {
                self.stats.total_timed_out.fetch_add(1, Ordering::Relaxed);
                Err(GovernorError::ExecutionTimeout {
                    request_id: RequestId(req_id),
                    tool: tool_name.to_string(),
                    timeout: self.timeout,
                })
            }
        }
    }

    /// Execute a non-abortable tool call under governor control.
    ///
    /// Use this for work dispatched to `spawn_blocking`: Tokio timeouts cannot
    /// stop an already-running blocking closure, so releasing permits/write gates
    /// on timeout would let later requests overlap with still-running work.
    pub async fn execute_non_abortable<F, T>(
        &self,
        tool_name: &str,
        fut: F,
    ) -> Result<T, GovernorError>
    where
        F: std::future::Future<Output = T>,
    {
        let weight = classify_tool(tool_name);
        let permits_needed = weight.permits();
        let req_id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let now = Instant::now();

        self.stats.total_submitted.fetch_add(1, Ordering::Relaxed);
        {
            let mut active = self.active.lock();
            active.insert(
                req_id,
                RequestEntry {
                    tool: tool_name.to_string(),
                    weight,
                    phase: RequestPhase::Queued,
                    submitted_at: now,
                    phase_started_at: now,
                },
            );
        }

        let permit = match tokio::time::timeout(
            self.queue_timeout,
            self.semaphore.acquire_many(permits_needed),
        )
        .await
        {
            Ok(Ok(permit)) => permit,
            Ok(Err(_closed)) => {
                self.remove_request(req_id);
                self.stats
                    .total_queue_rejected
                    .fetch_add(1, Ordering::Relaxed);
                return Err(GovernorError::SemaphoreClosed);
            }
            Err(_elapsed) => {
                self.remove_request(req_id);
                self.stats
                    .total_queue_rejected
                    .fetch_add(1, Ordering::Relaxed);
                return Err(GovernorError::QueueTimeout {
                    request_id: RequestId(req_id),
                    tool: tool_name.to_string(),
                    waited: self.queue_timeout,
                    weight,
                    in_flight: self.active_count(),
                });
            }
        };

        let result = if weight.needs_write_gate() {
            let _gate =
                match tokio::time::timeout(self.queue_timeout, self.write_gate.write()).await {
                    Ok(guard) => guard,
                    Err(_elapsed) => {
                        self.remove_request(req_id);
                        drop(permit);
                        self.stats
                            .total_queue_rejected
                            .fetch_add(1, Ordering::Relaxed);
                        return Err(GovernorError::WriteGateTimeout {
                            request_id: RequestId(req_id),
                            tool: tool_name.to_string(),
                        });
                    }
                };
            self.transition_to_executing(req_id);
            fut.await
        } else {
            let _gate = match tokio::time::timeout(self.queue_timeout, self.write_gate.read()).await
            {
                Ok(guard) => guard,
                Err(_elapsed) => {
                    self.remove_request(req_id);
                    drop(permit);
                    self.stats
                        .total_queue_rejected
                        .fetch_add(1, Ordering::Relaxed);
                    return Err(GovernorError::WriteGateTimeout {
                        request_id: RequestId(req_id),
                        tool: tool_name.to_string(),
                    });
                }
            };
            self.transition_to_executing(req_id);
            fut.await
        };

        self.remove_request(req_id);
        drop(permit);
        self.stats.total_completed.fetch_add(1, Ordering::Relaxed);
        Ok(result)
    }

    /// Transition a tracked request from Queued to Executing.
    fn transition_to_executing(&self, req_id: u64) {
        let mut active = self.active.lock();
        if let Some(entry) = active.get_mut(&req_id) {
            entry.phase = RequestPhase::Executing;
            entry.phase_started_at = Instant::now();
        }
        let executing = active
            .values()
            .filter(|e| e.phase == RequestPhase::Executing)
            .count() as u64;
        self.stats
            .peak_in_flight
            .fetch_max(executing, Ordering::Relaxed);
    }

    fn remove_request(&self, id: u64) {
        let mut active = self.active.lock();
        active.remove(&id);
    }

    fn active_count(&self) -> usize {
        self.active.lock().len()
    }
}

impl Default for RequestGovernor {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors from the governor layer.
#[derive(Debug, thiserror::Error)]
pub enum GovernorError {
    #[error(
        "[{request_id}] tool '{tool}' timed out waiting for capacity ({waited:?}); \
         weight={weight:?}, {in_flight} requests in-flight — retry in a moment"
    )]
    QueueTimeout {
        request_id: RequestId,
        tool: String,
        waited: Duration,
        weight: ToolWeight,
        in_flight: usize,
    },

    #[error(
        "[{request_id}] tool '{tool}' execution timed out after {timeout:?} — \
         the operation may still be running in the background"
    )]
    ExecutionTimeout {
        request_id: RequestId,
        tool: String,
        timeout: Duration,
    },

    #[error(
        "[{request_id}] tool '{tool}' timed out waiting for write gate — \
         a heavy operation is running"
    )]
    WriteGateTimeout { request_id: RequestId, tool: String },

    #[error("governor semaphore closed — server is shutting down")]
    SemaphoreClosed,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_tool_weights() {
        assert_eq!(classify_tool("get_symbol"), ToolWeight::Light);
        assert_eq!(classify_tool("search_text"), ToolWeight::Light);
        assert_eq!(classify_tool("get_file_context"), ToolWeight::Light);
        assert_eq!(classify_tool("batch_edit"), ToolWeight::Heavy);
        assert_eq!(classify_tool("batch_rename"), ToolWeight::Heavy);
        assert_eq!(classify_tool("index_folder"), ToolWeight::Heavy);
        assert_eq!(classify_tool("analyze_file_impact"), ToolWeight::Medium);
        assert_eq!(classify_tool("replace_symbol_body"), ToolWeight::Heavy);
    }

    #[test]
    fn test_heavy_ops_need_write_gate() {
        assert!(ToolWeight::Heavy.needs_write_gate());
        assert!(!ToolWeight::Medium.needs_write_gate());
        assert!(!ToolWeight::Light.needs_write_gate());
    }

    #[test]
    fn test_request_id_display() {
        assert_eq!(format!("{}", RequestId(42)), "req-42");
    }

    #[test]
    fn test_governor_snapshot_starts_empty() {
        let gov = RequestGovernor::new();
        let snap = gov.snapshot();
        assert_eq!(snap.max_concurrency, DEFAULT_MAX_CONCURRENCY);
        assert_eq!(snap.available_permits, DEFAULT_MAX_CONCURRENCY);
        assert!(snap.in_flight.is_empty());
        assert_eq!(snap.total_submitted, 0);
        assert_eq!(snap.total_completed, 0);
    }

    #[tokio::test]
    async fn test_governor_assigns_request_ids() {
        let gov = RequestGovernor::new();
        let r1 = gov.execute("get_symbol", async { 1 }).await.unwrap();
        let r2 = gov.execute("search_text", async { 2 }).await.unwrap();
        assert_eq!(r1, 1);
        assert_eq!(r2, 2);

        let snap = gov.snapshot();
        assert_eq!(snap.total_submitted, 2);
        assert_eq!(snap.total_completed, 2);
        assert!(snap.in_flight.is_empty());
    }

    #[tokio::test]
    async fn test_governor_execution_timeout() {
        let gov =
            RequestGovernor::with_config(8, Duration::from_millis(50), Duration::from_secs(5));
        let result = gov
            .execute("get_symbol", async {
                tokio::time::sleep(Duration::from_millis(200)).await;
                42
            })
            .await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            GovernorError::ExecutionTimeout { .. }
        ));

        let snap = gov.snapshot();
        assert_eq!(snap.total_timed_out, 1);
        assert!(
            snap.in_flight.is_empty(),
            "timed-out request should be cleaned up"
        );
    }

    #[tokio::test]
    async fn test_governor_non_abortable_waits_past_execution_timeout() {
        let gov =
            RequestGovernor::with_config(8, Duration::from_millis(10), Duration::from_secs(5));
        let result = gov
            .execute_non_abortable("get_symbol", async {
                tokio::time::sleep(Duration::from_millis(30)).await;
                42
            })
            .await;

        assert_eq!(result.unwrap(), 42);
        let snap = gov.snapshot();
        assert_eq!(snap.total_completed, 1);
        assert_eq!(snap.total_timed_out, 0);
        assert!(snap.in_flight.is_empty());
    }

    #[tokio::test]
    async fn test_governor_queue_timeout() {
        // Only 2 permits, heavy op needs 3 → can never acquire → queue timeout
        let gov =
            RequestGovernor::with_config(2, Duration::from_secs(5), Duration::from_millis(50));
        let result = gov.execute("batch_rename", async { 42 }).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, GovernorError::QueueTimeout { .. }));

        let snap = gov.snapshot();
        assert_eq!(snap.total_queue_rejected, 1);
    }

    #[tokio::test]
    async fn test_governor_tracks_in_flight() {
        let gov = RequestGovernor::with_config(8, Duration::from_secs(5), Duration::from_secs(5));
        let gov_clone = gov.clone();

        let (tx, rx) = tokio::sync::oneshot::channel::<()>();

        let handle = tokio::spawn(async move {
            gov_clone
                .execute("search_text", async {
                    rx.await.ok();
                    42
                })
                .await
        });

        // Give the task time to start executing
        tokio::time::sleep(Duration::from_millis(20)).await;

        let snap = gov.snapshot();
        assert_eq!(snap.in_flight.len(), 1);
        assert_eq!(snap.in_flight[0].tool, "search_text");
        assert_eq!(snap.in_flight[0].phase, RequestPhase::Executing);
        assert!(snap.in_flight[0].age_ms >= 10);

        // Release the task
        tx.send(()).ok();
        let result = handle.await.unwrap();
        assert_eq!(result.unwrap(), 42);

        let snap = gov.snapshot();
        assert!(snap.in_flight.is_empty());
        assert_eq!(snap.peak_in_flight, 1);
    }

    #[tokio::test]
    async fn test_governor_concurrent_reads_allowed() {
        let gov = RequestGovernor::with_config(4, Duration::from_secs(5), Duration::from_secs(5));

        // 3 concurrent light reads should all succeed
        let handles: Vec<_> = (0..3)
            .map(|i| {
                let g = gov.clone();
                tokio::spawn(async move {
                    g.execute("get_symbol", async move {
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        i
                    })
                    .await
                })
            })
            .collect();

        for h in handles {
            assert!(h.await.unwrap().is_ok());
        }

        assert_eq!(gov.snapshot().total_completed, 3);
    }

    #[tokio::test]
    async fn acquire_request_slot_bounds_concurrency_and_sheds_when_full() {
        // P2-F: the `/mcp` HTTP boundary acquires one permit per request via
        // `acquire_request_slot`. With a single-permit governor and a tiny queue
        // timeout, a second concurrent acquire (while the first permit is held)
        // is shed with QueueTimeout — proving concurrent clients are bounded.
        let gov = Arc::new(RequestGovernor::with_config(
            1,
            Duration::from_secs(5),
            Duration::from_millis(50),
        ));

        let held = gov
            .acquire_request_slot()
            .await
            .expect("first slot acquires");
        assert_eq!(gov.available_permits(), 0, "permit is held");

        // Second acquire finds no capacity and times out (shed → 503).
        let shed = gov.acquire_request_slot().await;
        assert!(
            matches!(shed, Err(GovernorError::QueueTimeout { .. })),
            "saturated governor must shed the second request, got {shed:?}"
        );

        // Releasing the held permit frees capacity for the next request.
        drop(held);
        let next = gov.acquire_request_slot().await;
        assert!(next.is_ok(), "permit re-acquirable after release");
        assert_eq!(gov.available_permits(), 0);
        drop(next);
        assert_eq!(gov.available_permits(), 1, "permit released on drop");
    }
}
