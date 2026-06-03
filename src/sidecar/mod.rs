pub mod governor;
pub mod handlers;
pub mod port_file;
pub mod router;
pub mod server;

pub use governor::RequestGovernor;
pub use server::spawn_sidecar;

use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use serde::Serialize;

use crate::live_index::store::SharedIndex;

/// Handle returned by `spawn_sidecar`. Dropping this or sending on `shutdown_tx`
/// gracefully stops the background axum server and cleans up port/PID files.
///
/// Prefer `shutdown_and_join().await` over a bare `shutdown_tx.send(())`: the
/// helper awaits the server task's completion so the listener is fully dropped
/// before the caller proceeds. This matters in tests where rapid teardown +
/// rebind cycles must not race a still-open listener.
pub struct SidecarHandle {
    /// The ephemeral port the sidecar bound to.
    pub port: u16,
    /// Send `()` on this channel to initiate graceful shutdown.
    pub shutdown_tx: tokio::sync::oneshot::Sender<()>,
    /// Join handle for the spawned axum-serve task. Awaiting this after sending
    /// on `shutdown_tx` guarantees the listener has been dropped.
    pub server_join: tokio::task::JoinHandle<()>,
    /// Shared token stats for the sidecar session.
    /// Pass this `Arc` to `SymForgeServer::new()` so the health tool can report savings.
    pub token_stats: Arc<TokenStats>,
}

impl SidecarHandle {
    /// Signal graceful shutdown and await server-task completion.
    ///
    /// Consumes the handle; the listener and port/PID files are fully released
    /// by the time this returns. Tests should prefer this over a bare
    /// `shutdown_tx.send(())` followed by an arbitrary sleep.
    pub async fn shutdown_and_join(self) {
        let _ = self.shutdown_tx.send(());
        let _ = self.server_join.await;
    }
}

// ---------------------------------------------------------------------------
// TokenStats — per-session atomic counters for hook fire counts and token savings
// ---------------------------------------------------------------------------

/// Lightweight snapshot of all token stats counters. Returned by `/stats`.
#[derive(Debug, Clone, Serialize)]
pub struct StatsSnapshot {
    pub read_fires: usize,
    pub read_saved_tokens: u64,
    pub edit_fires: usize,
    pub edit_saved_tokens: u64,
    pub write_fires: usize,
    pub grep_fires: usize,
    pub grep_saved_tokens: u64,
}

/// In-memory atomic counters tracking hook fires and estimated token savings per hook type.
///
/// All counters start at zero; incremented by handlers on each successful response.
/// Token savings are estimated as `(file_bytes - output_bytes) / 4` (bytes-per-token heuristic).
pub struct TokenStats {
    pub read_fires: AtomicUsize,
    pub read_saved_tokens: AtomicU64,
    pub edit_fires: AtomicUsize,
    pub edit_saved_tokens: AtomicU64,
    /// Write fires track new-file indexing; no savings because it's additive context.
    pub write_fires: AtomicUsize,
    pub grep_fires: AtomicUsize,
    pub grep_saved_tokens: AtomicU64,
    /// Per-tool invocation counts since daemon start.
    pub tool_calls: Mutex<HashMap<String, usize>>,
    /// Per-tool token tracking: tool_name -> (tokens_served, tokens_saved).
    pub tool_token_details: Mutex<HashMap<String, (u64, u64)>>,
    /// Total tokens served across all tool calls this session.
    pub total_tokens_served: AtomicU64,
    /// Total estimated naive-equivalent tokens (what raw file reads would cost).
    pub total_tokens_naive: AtomicU64,
}

impl TokenStats {
    /// Create a new `Arc<TokenStats>` with all counters at zero.
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            read_fires: AtomicUsize::new(0),
            read_saved_tokens: AtomicU64::new(0),
            edit_fires: AtomicUsize::new(0),
            edit_saved_tokens: AtomicU64::new(0),
            write_fires: AtomicUsize::new(0),
            grep_fires: AtomicUsize::new(0),
            grep_saved_tokens: AtomicU64::new(0),
            tool_calls: Mutex::new(HashMap::new()),
            tool_token_details: Mutex::new(HashMap::new()),
            total_tokens_served: AtomicU64::new(0),
            total_tokens_naive: AtomicU64::new(0),
        })
    }

    /// Record a Read hook fire. Savings = (file_bytes - output_bytes) / 4.
    pub fn record_read(&self, file_bytes: u64, output_bytes: u64) {
        self.read_fires.fetch_add(1, Ordering::Relaxed);
        let saved = file_bytes.saturating_sub(output_bytes) / 4;
        self.read_saved_tokens.fetch_add(saved, Ordering::Relaxed);
    }

    /// Record an Edit hook fire. Savings = (file_bytes - output_bytes) / 4.
    pub fn record_edit(&self, file_bytes: u64, output_bytes: u64) {
        self.edit_fires.fetch_add(1, Ordering::Relaxed);
        let saved = file_bytes.saturating_sub(output_bytes) / 4;
        self.edit_saved_tokens.fetch_add(saved, Ordering::Relaxed);
    }

    /// Record a Write hook fire (new-file indexing). No savings — additive context.
    pub fn record_write(&self) {
        self.write_fires.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a Grep hook fire. Savings = (file_bytes - output_bytes) / 4.
    pub fn record_grep(&self, file_bytes: u64, output_bytes: u64) {
        self.grep_fires.fetch_add(1, Ordering::Relaxed);
        let saved = file_bytes.saturating_sub(output_bytes) / 4;
        self.grep_saved_tokens.fetch_add(saved, Ordering::Relaxed);
    }

    /// Record a single MCP tool invocation by name.
    pub fn record_tool_call(&self, name: &str) {
        let mut map = self.tool_calls.lock();
        *map.entry(name.to_string()).or_insert(0) += 1;
    }

    /// Record per-tool token details: tokens served and tokens saved.
    pub fn record_tool_tokens(&self, tool_name: &str, tokens_served: u64, tokens_saved: u64) {
        let mut map = self.tool_token_details.lock();
        let entry = map.entry(tool_name.to_string()).or_insert((0, 0));
        entry.0 += tokens_served;
        entry.1 += tokens_saved;
        self.total_tokens_served
            .fetch_add(tokens_served, Ordering::Relaxed);
        self.total_tokens_naive
            .fetch_add(tokens_served + tokens_saved, Ordering::Relaxed);
    }

    /// Return per-tool token details sorted by tokens saved descending.
    pub fn tool_token_details(&self) -> Vec<(String, u64, u64)> {
        let map = self.tool_token_details.lock();
        let mut details: Vec<(String, u64, u64)> = map
            .iter()
            .map(|(k, (served, saved))| (k.clone(), *served, *saved))
            .collect();
        details.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.cmp(&b.0)));
        details
    }

    /// Return per-tool invocation counts sorted by count descending, then name ascending.
    pub fn tool_call_counts(&self) -> Vec<(String, usize)> {
        let map = self.tool_calls.lock();
        let mut counts: Vec<(String, usize)> = map.iter().map(|(k, v)| (k.clone(), *v)).collect();
        counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        counts
    }

    /// Read all counter values atomically (Relaxed ordering — display only).
    pub fn summary(&self) -> StatsSnapshot {
        StatsSnapshot {
            read_fires: self.read_fires.load(Ordering::Relaxed),
            read_saved_tokens: self.read_saved_tokens.load(Ordering::Relaxed),
            edit_fires: self.edit_fires.load(Ordering::Relaxed),
            edit_saved_tokens: self.edit_saved_tokens.load(Ordering::Relaxed),
            write_fires: self.write_fires.load(Ordering::Relaxed),
            grep_fires: self.grep_fires.load(Ordering::Relaxed),
            grep_saved_tokens: self.grep_saved_tokens.load(Ordering::Relaxed),
        }
    }
}

// ---------------------------------------------------------------------------
// SymbolSnapshot — lightweight copy of a symbol for pre/post diff in impact handler
// ---------------------------------------------------------------------------

/// Lightweight snapshot of a symbol used to detect pre/post-edit changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolSnapshot {
    pub name: String,
    pub kind: String,
    pub line_range: (u32, u32),
    pub byte_range: (u32, u32),
}

// ---------------------------------------------------------------------------
// SidecarState — bundles index + stats + symbol cache for all handlers
// ---------------------------------------------------------------------------

/// Axum state type bundling the shared index, token statistics, and pre-edit symbol cache.
///
/// Passed to every handler via `State<SidecarState>`. Replaces bare `SharedIndex` as the
/// axum state type in Plan 06-01.
#[derive(Clone)]
pub struct SidecarState {
    pub index: SharedIndex,
    pub token_stats: Arc<TokenStats>,
    /// Canonical project root for file-system reads during impact analysis.
    /// `None` falls back to process cwd for local test setups.
    pub repo_root: Option<PathBuf>,
    /// Per-file symbol snapshot cache for impact diff.
    /// Key: relative file path. Value: symbol list captured before last edit.
    pub symbol_cache: Arc<RwLock<HashMap<String, Vec<SymbolSnapshot>>>>,
}

// ---------------------------------------------------------------------------
// build_with_budget — truncate a list of lines at a byte boundary
// ---------------------------------------------------------------------------

/// Join `items` with newlines, stopping before exceeding `max_bytes`.
///
/// If `max_bytes` is 0, all items are returned without truncation.
///
/// Returns `(text, remaining_count)` where `remaining_count` is 0 when no
/// truncation occurred. A canonical truncation suffix is appended when
/// items were dropped.
pub fn build_with_budget(items: &[String], max_bytes: u64) -> (String, usize) {
    if max_bytes == 0 || items.is_empty() {
        return (items.join("\n"), 0);
    }

    let mut included = Vec::new();
    let mut used_bytes: u64 = 0;

    for (i, item) in items.iter().enumerate() {
        // Each item costs: len + 1 newline (except the last).
        let item_cost = item.len() as u64 + if i + 1 < items.len() { 1 } else { 0 };
        if used_bytes + item_cost > max_bytes && !included.is_empty() {
            // Would exceed budget — stop here.
            let remaining = items.len() - included.len();
            let mut text = included.join("\n");
            text.push_str(&budget_truncation_suffix(max_bytes, remaining));
            return (text, remaining);
        }
        used_bytes += item_cost;
        included.push(item.as_str());
    }

    // After the loop: if fewer items were included than available (e.g. because
    // the very first item exceeded max_bytes and forced inclusion while the rest
    // were silently dropped), always append the truncation suffix so callers
    // know output was cut short.
    if included.len() < items.len() {
        let remaining = items.len() - included.len();
        let mut text = included.join("\n");
        text.push_str(&budget_truncation_suffix(max_bytes, remaining));
        return (text, remaining);
    }

    (included.join("\n"), 0)
}

const CANONICAL_TRUNCATION_MARKER: &str = "[truncated]";
const APPROX_BYTES_PER_TOKEN: u64 = 4;

fn approx_tokens_from_bytes(bytes: u64) -> u64 {
    bytes.saturating_add(APPROX_BYTES_PER_TOKEN - 1) / APPROX_BYTES_PER_TOKEN
}

fn budget_truncation_suffix(max_bytes: u64, remaining: usize) -> String {
    let max_tokens = approx_tokens_from_bytes(max_bytes);
    format!(
        "\n{CANONICAL_TRUNCATION_MARKER} Truncated at ~{max_tokens} tokens. {remaining} additional output line(s) not shown."
    )
}

// ---------------------------------------------------------------------------
// Unit tests for Task 1
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- TokenStats ---

    #[test]
    fn test_token_stats_new_all_zeros() {
        let stats = TokenStats::new();
        let snap = stats.summary();
        assert_eq!(snap.read_fires, 0);
        assert_eq!(snap.read_saved_tokens, 0);
        assert_eq!(snap.edit_fires, 0);
        assert_eq!(snap.edit_saved_tokens, 0);
        assert_eq!(snap.write_fires, 0);
        assert_eq!(snap.grep_fires, 0);
        assert_eq!(snap.grep_saved_tokens, 0);
    }

    #[test]
    fn test_token_stats_record_read() {
        let stats = TokenStats::new();
        stats.record_read(1000, 200);
        let snap = stats.summary();
        assert_eq!(snap.read_fires, 1);
        // (1000 - 200) / 4 = 200
        assert_eq!(snap.read_saved_tokens, 200);
    }

    #[test]
    fn test_token_stats_record_edit() {
        let stats = TokenStats::new();
        stats.record_edit(800, 300);
        let snap = stats.summary();
        assert_eq!(snap.edit_fires, 1);
        // (800 - 300) / 4 = 125
        assert_eq!(snap.edit_saved_tokens, 125);
    }

    #[test]
    fn test_token_stats_record_write_no_savings() {
        let stats = TokenStats::new();
        stats.record_write();
        let snap = stats.summary();
        assert_eq!(snap.write_fires, 1);
        // No savings fields for write
    }

    #[test]
    fn test_token_stats_record_grep() {
        let stats = TokenStats::new();
        stats.record_grep(2000, 100);
        let snap = stats.summary();
        assert_eq!(snap.grep_fires, 1);
        // (2000 - 100) / 4 = 475
        assert_eq!(snap.grep_saved_tokens, 475);
    }

    #[test]
    fn test_token_stats_records_tool_calls() {
        let stats = TokenStats::new();

        stats.record_tool_call("search_text");
        stats.record_tool_call("search_text");
        stats.record_tool_call("get_file_context");

        let counts = stats.tool_call_counts();

        // search_text has the highest count and should appear first.
        assert_eq!(counts[0], ("search_text".to_string(), 2));
        assert_eq!(counts[1], ("get_file_context".to_string(), 1));
        assert_eq!(counts.len(), 2);
    }

    #[test]
    fn test_token_stats_saturating_sub_no_underflow() {
        // output_bytes > file_bytes — should not underflow
        let stats = TokenStats::new();
        stats.record_read(100, 500);
        let snap = stats.summary();
        assert_eq!(
            snap.read_saved_tokens, 0,
            "saturating_sub prevents underflow"
        );
    }

    #[test]
    fn test_token_stats_accumulates_multiple_fires() {
        let stats = TokenStats::new();
        stats.record_read(1000, 200);
        stats.record_read(800, 400);
        let snap = stats.summary();
        assert_eq!(snap.read_fires, 2);
        // 200 + 100 = 300
        assert_eq!(snap.read_saved_tokens, 300);
    }

    // --- build_with_budget ---

    #[test]
    fn test_build_with_budget_no_truncation_when_fits() {
        let items: Vec<String> = vec![
            "line1".to_string(),
            "line2".to_string(),
            "line3".to_string(),
        ];
        let (text, remaining) = build_with_budget(&items, 1000);
        assert_eq!(text, "line1\nline2\nline3");
        assert_eq!(remaining, 0, "no truncation when all items fit");
    }

    #[test]
    fn test_build_with_budget_truncates_at_logical_boundary() {
        // "line1\nline2\nline3" = 5+1+5+1+5 = 17 bytes
        // max_bytes=12 means line1(5+1=6) + line2(5+1=6) = 12 bytes fits,
        // but adding line3(5) would be 12+5=17 > 12
        let items: Vec<String> = vec![
            "line1".to_string(),
            "line2".to_string(),
            "line3".to_string(),
        ];
        let (text, remaining) = build_with_budget(&items, 12);
        assert!(text.contains("line1"), "line1 should be included");
        assert!(text.contains("line2"), "line2 should be included");
        assert!(text.contains("truncated"), "should have truncation suffix");
        assert_eq!(remaining, 1, "1 item was truncated");
    }

    #[test]
    fn test_build_with_budget_zero_means_unlimited() {
        let items: Vec<String> = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let (text, remaining) = build_with_budget(&items, 0);
        assert_eq!(text, "a\nb\nc");
        assert_eq!(remaining, 0);
    }

    #[test]
    fn test_build_with_budget_empty_items() {
        let items: Vec<String> = vec![];
        let (text, remaining) = build_with_budget(&items, 100);
        assert_eq!(text, "");
        assert_eq!(remaining, 0);
    }

    #[test]
    fn test_build_with_budget_truncation_suffix_format() {
        let items: Vec<String> = vec![
            "line1".to_string(),
            "line2".to_string(),
            "line3".to_string(),
            "line4".to_string(),
        ];
        let (text, remaining) = build_with_budget(&items, 12);
        assert!(
            text.ends_with(
                "[truncated] Truncated at ~3 tokens. 2 additional output line(s) not shown."
            ),
            "suffix should use canonical truncation marker and mention 2 remaining items, got: {text}"
        );
        assert_eq!(remaining, 2);
    }

    // --- SidecarState construction ---

    #[test]
    fn test_sidecar_state_constructs() {
        use crate::live_index::store::{CircuitBreakerState, LiveIndex};
        use parking_lot::RwLock;
        use std::collections::HashMap;
        use std::sync::Arc;
        use std::time::{Duration, Instant, SystemTime};

        let index = crate::live_index::SharedIndexHandle::shared(LiveIndex {
            files: HashMap::new(),
            loaded_at: Instant::now(),
            loaded_at_system: SystemTime::now(),
            load_duration: Duration::ZERO,
            cb_state: CircuitBreakerState::new(0.20),
            is_empty: true,
            load_source: crate::live_index::store::IndexLoadSource::EmptyBootstrap,
            snapshot_verify_state: crate::live_index::store::SnapshotVerifyState::NotNeeded,
            reverse_index: HashMap::new(),
            files_by_basename: HashMap::new(),
            files_by_dir_component: HashMap::new(),
            trigram_index: crate::live_index::trigram::TrigramIndex::new(),
            gitignore: None,
            skipped_files: Vec::new(),
            coupling_store: None,
            local_empty_reason: std::sync::Arc::new(parking_lot::RwLock::new(None)),
            indexed_root: None,
        });

        let state = SidecarState {
            index,
            token_stats: TokenStats::new(),
            repo_root: None,
            symbol_cache: Arc::new(RwLock::new(HashMap::new())),
        };

        // Clone must work (derived Clone)
        let _cloned = state.clone();
    }
}
