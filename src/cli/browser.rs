//! `BrowserOpener` seam — open the dashboard URL in the OS browser (009, D4).
//!
//! The real impl shells the OS opener via `std::process::Command`
//! (`cmd /c start` | `open` | `xdg-open`); headless/no-opener prints the URL and
//! skips (never an error). Tests use a no-op opener that records the URL. No new
//! dependency (no `open`/`webbrowser` crate) — D4 / ponytail rung 4.
//!
//! Phase 1 (T003) is a compiling skeleton: the `BrowserOpener` trait, the real
//! OS-opener impl, and `NoopBrowserOpener` land in Phase 3 (Foundational, T010).
//! Logic is intentionally deferred here.

/// Outcome of a browser-open attempt.
///
/// `Opened` means the OS opener was invoked; `Skipped` means the environment is
/// headless / has no opener and the URL was printed instead (never an error).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserOpenOutcome {
    /// The OS opener was invoked for the URL.
    Opened,
    /// No opener available (headless); the URL was printed and the open skipped.
    Skipped,
}
