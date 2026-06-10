//! Watcher state snapshot types — pure data shared by the engine (health stats)
//! and the server-only notify-based watcher runtime. No server dependencies, so
//! these compile in `--no-default-features --features embed` builds.

use std::time::SystemTime;

/// Watcher operational state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WatcherState {
    /// File watcher is registered and looping (receiving/ready for events).
    Active,
    /// Watcher (re)start has been initiated but the recursive filesystem watch
    /// has not finished registering yet. On large trees, registering the
    /// `notify` recursive watch can take seconds; this state distinguishes
    /// in-progress startup from a watcher that is genuinely not running (`Off`).
    Starting,
    /// File watcher encountered errors but partial operation continues.
    Degraded,
    /// File watcher is not running.
    Off,
}

/// Snapshot of file watcher status for health reporting.
#[derive(Clone, Debug)]
pub struct WatcherInfo {
    pub state: WatcherState,
    pub events_processed: u64,
    pub last_event_at: Option<SystemTime>,
    pub debounce_window_ms: u64,
    /// Number of watcher buffer overflow events detected.
    pub overflow_count: u64,
    /// Wall-clock time of the most recent overflow event.
    pub last_overflow_at: Option<SystemTime>,
    /// Cumulative count of stale files found and re-indexed by reconciliation sweeps.
    pub stale_files_found: u64,
    /// Wall-clock time of the most recent reconciliation sweep.
    pub last_reconcile_at: Option<SystemTime>,
}

impl Default for WatcherInfo {
    fn default() -> Self {
        WatcherInfo {
            state: WatcherState::Off,
            events_processed: 0,
            last_event_at: None,
            debounce_window_ms: 200,
            overflow_count: 0,
            last_overflow_at: None,
            stale_files_found: 0,
            last_reconcile_at: None,
        }
    }
}

impl WatcherInfo {
    pub fn detached_local_fallback() -> Self {
        WatcherInfo {
            debounce_window_ms: 0,
            ..WatcherInfo::default()
        }
    }

    pub fn is_local_fallback(&self) -> bool {
        matches!(self.state, WatcherState::Off)
            && self.events_processed == 0
            && self.last_event_at.is_none()
            && self.debounce_window_ms == 0
            && self.overflow_count == 0
            && self.last_overflow_at.is_none()
            && self.stale_files_found == 0
            && self.last_reconcile_at.is_none()
    }
}
