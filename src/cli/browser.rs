//! `BrowserOpener` seam — open the dashboard URL in the OS browser (009, D4).
//!
//! The real impl shells the OS opener via `std::process::Command`
//! (`cmd /c start` | `open` | `xdg-open`); headless/no-opener prints the URL and
//! skips (never an error). Tests use a no-op opener that records the URL. No new
//! dependency (no `open`/`webbrowser` crate) — D4 / ponytail rung 4.

use std::process::Stdio;

/// Outcome of a browser-open attempt.
///
/// `Opened` means the OS opener was invoked; `Skipped` means the environment is
/// headless / has no opener and the URL was printed instead (never an error).
//
// Naming note: data-model E6 / contracts/seams.md call this `OpenOutcome`; the
// Phase-1 skeleton shipped it as `BrowserOpenOutcome` (more descriptive, no other
// references), so we keep that name to avoid a gratuitous rename.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserOpenOutcome {
    /// The OS opener was invoked for the URL.
    Opened,
    /// No opener available (headless); the URL was printed and the open skipped.
    Skipped,
}

/// Seam for opening a URL in the operator's browser (E6, contracts/seams.md).
///
/// The real impl performs OS I/O; the test impl records and opens nothing, so the
/// wizard/admin flow stays side-effect-free in tests (FR-017).
pub trait BrowserOpener {
    /// Attempt to open `url`. Returns [`BrowserOpenOutcome::Opened`] when the OS
    /// opener was invoked, or [`BrowserOpenOutcome::Skipped`] when headless / no
    /// opener is available. Never returns an error — a failed open degrades to
    /// `Skipped` (FR-011).
    fn open_url(&self, url: &str) -> BrowserOpenOutcome;
}

/// Real OS browser opener: shells the platform opener via `std::process::Command`.
///
/// - Windows: `cmd /c start "" <url>` (the empty `""` is the window-title arg
///   `start` consumes, so the URL is not mistaken for a title).
/// - macOS: `open <url>`.
/// - Linux/other Unix: `xdg-open <url>`, but **only** when a graphical session is
///   present (`DISPLAY` or `WAYLAND_DISPLAY` set); a headless box skips.
///
/// A missing command, a spawn error, or a non-zero exit all degrade to
/// [`BrowserOpenOutcome::Skipped`] — opening a browser is a convenience, never a
/// hard requirement (FR-011). This type prints nothing; the caller decides
/// messaging (it has the URL and the outcome).
#[derive(Debug, Default, Clone, Copy)]
pub struct OsBrowserOpener;

impl BrowserOpener for OsBrowserOpener {
    fn open_url(&self, url: &str) -> BrowserOpenOutcome {
        let spawned = if cfg!(target_os = "windows") {
            // `start` is a cmd builtin, so it must be invoked through `cmd /c`.
            // The empty title arg keeps the URL from being parsed as the title.
            run_opener("cmd", &["/c", "start", "", url])
        } else if cfg!(target_os = "macos") {
            run_opener("open", &[url])
        } else if has_graphical_session() {
            run_opener("xdg-open", &[url])
        } else {
            // Headless Unix (no DISPLAY/WAYLAND_DISPLAY): never spawn an opener.
            false
        };

        if spawned {
            BrowserOpenOutcome::Opened
        } else {
            BrowserOpenOutcome::Skipped
        }
    }
}

/// Spawn `program args...` detached from this process's stdio, returning whether
/// the spawn itself succeeded. We do **not** wait for the opener to exit — a
/// browser launcher commonly stays resident — so success means "the opener
/// process started", which is the meaningful signal for `Opened` vs `Skipped`.
fn run_opener(program: &str, args: &[&str]) -> bool {
    crate::process_util::hidden_command(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .is_ok()
}

/// True when a graphical session appears available on Unix (some `DISPLAY` or
/// `WAYLAND_DISPLAY` is set). On Windows/macOS this is unused (those branches
/// always attempt the opener). Headless CI/containers set neither, so the opener
/// is skipped rather than erroring.
fn has_graphical_session() -> bool {
    let non_empty = |v: Result<String, _>| matches!(v, Ok(s) if !s.is_empty());
    non_empty(std::env::var("DISPLAY")) || non_empty(std::env::var("WAYLAND_DISPLAY"))
}

/// Test opener: records every URL it is asked to open and opens nothing, always
/// returning [`BrowserOpenOutcome::Skipped`]. Interior mutability (`RefCell`) lets
/// it record through the shared `&self` of [`BrowserOpener::open_url`].
#[derive(Debug, Default)]
pub struct NoopBrowserOpener {
    opened: std::cell::RefCell<Vec<String>>,
}

impl NoopBrowserOpener {
    /// The URLs `open_url` was called with, in order.
    pub fn opened_urls(&self) -> Vec<String> {
        self.opened.borrow().clone()
    }
}

impl BrowserOpener for NoopBrowserOpener {
    fn open_url(&self, url: &str) -> BrowserOpenOutcome {
        self.opened.borrow_mut().push(url.to_string());
        BrowserOpenOutcome::Skipped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_opener_records_url_and_skips() {
        let opener = NoopBrowserOpener::default();
        let outcome = opener.open_url("http://127.0.0.1:8787/admin");
        // Never opens a real browser: always Skipped, URL recorded.
        assert_eq!(outcome, BrowserOpenOutcome::Skipped);
        assert_eq!(opener.opened_urls(), vec!["http://127.0.0.1:8787/admin"]);
    }

    #[test]
    fn noop_opener_records_multiple_urls_in_order() {
        let opener = NoopBrowserOpener::default();
        let _ = opener.open_url("http://a/admin");
        let _ = opener.open_url("http://b/admin");
        assert_eq!(
            opener.opened_urls(),
            vec!["http://a/admin", "http://b/admin"]
        );
    }
}
