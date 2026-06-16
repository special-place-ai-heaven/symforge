//! First-run / post-update onboarding banner with persisted shown-state.
//!
//! After install/update or `serve` startup, the operator sees a one-time banner
//! with the attach URL (and a browser-open offer). The shown version is
//! recorded so the banner does not repeat until the SymForge build version
//! changes (spec FR-009 / SC-006).
//!
//! Everything here is injectable for tests: the state path and the current
//! version are parameters, and the browser-open side effect goes through an
//! `OnboardingSink` so tests never launch a real browser.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// On-disk filename for onboarding state inside the SymForge data dir.
pub const ONBOARDING_STATE_FILE: &str = "onboarding.json";

/// Persisted record of whether/when the onboarding banner was last shown.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct OnboardingState {
    /// The SymForge build version the banner was last shown for, if ever.
    #[serde(default)]
    pub last_shown_version: Option<String>,
}

impl OnboardingState {
    /// Load state from `path`. A missing or unparseable file yields the default
    /// (never-shown) state — onboarding is best-effort and must not fail the
    /// caller.
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Persist this state to `path` (parent dir created if needed).
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, json)
    }

    /// True when the banner should be shown for `current_version`.
    pub fn should_show(&self, current_version: &str) -> bool {
        self.last_shown_version.as_deref() != Some(current_version)
    }
}

/// Side-effect sink for the banner: where text is emitted and whether a browser
/// is opened. Injectable so tests observe behavior without touching stderr or a
/// real browser.
pub trait OnboardingSink {
    /// Emit a line of banner text.
    fn line(&mut self, text: &str);
    /// Offer to open `url` in a browser. Returns whether it was opened.
    fn offer_open(&mut self, url: &str) -> bool;
}

/// Default sink: prints to stderr and does **not** auto-open a browser (the
/// offer is shown as text; an interactive open is a later GUI concern). Keeping
/// the default non-interactive avoids surprising automation/CI.
pub struct StderrSink;

impl OnboardingSink for StderrSink {
    fn line(&mut self, text: &str) {
        eprintln!("{text}");
    }

    fn offer_open(&mut self, url: &str) -> bool {
        eprintln!("  Open it in your browser: {url}");
        false
    }
}

/// Resolve the onboarding-state path under the SymForge data dir for `base`.
pub fn state_path(base: &Path) -> PathBuf {
    crate::paths::resolve_symforge_dir(base).join(ONBOARDING_STATE_FILE)
}

/// Show the onboarding banner once per version. Loads state from `state_path`,
/// and if the banner has not been shown for `current_version`, renders it via
/// `sink` and records the version. Returns whether the banner was shown.
///
/// Best-effort: a state read/write failure never aborts the caller (install /
/// update / serve continue regardless).
pub fn maybe_show_banner(
    state_path: &Path,
    current_version: &str,
    attach_url: &str,
    sink: &mut impl OnboardingSink,
) -> bool {
    let state = OnboardingState::load(state_path);
    if !state.should_show(current_version) {
        return false;
    }

    render_banner(current_version, attach_url, sink);

    let next = OnboardingState {
        last_shown_version: Some(current_version.to_string()),
    };
    if let Err(e) = next.save(state_path) {
        // Do not fail the caller; just note it.
        sink.line(&format!("  (note: could not record onboarding state: {e})"));
    }
    true
}

fn render_banner(version: &str, attach_url: &str, sink: &mut impl OnboardingSink) {
    sink.line("");
    sink.line(&format!("SymForge {version} is ready."));
    sink.line(&format!(
        "  Attach an MCP client to this server: {attach_url}"
    ));
    sink.line("  Auto-configure your harnesses:  symforge init --scan");
    sink.offer_open(attach_url);
    sink.line("");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test sink: records emitted lines and offered URLs; never opens anything.
    #[derive(Default)]
    struct RecordingSink {
        lines: Vec<String>,
        offered: Vec<String>,
    }

    impl OnboardingSink for RecordingSink {
        fn line(&mut self, text: &str) {
            self.lines.push(text.to_string());
        }
        fn offer_open(&mut self, url: &str) -> bool {
            self.offered.push(url.to_string());
            false
        }
    }

    #[test]
    fn should_show_when_never_shown() {
        let s = OnboardingState::default();
        assert!(s.should_show("1.0.0"));
    }

    #[test]
    fn should_not_show_for_same_version() {
        let s = OnboardingState {
            last_shown_version: Some("1.0.0".to_string()),
        };
        assert!(!s.should_show("1.0.0"));
        assert!(s.should_show("1.0.1"));
    }

    #[test]
    fn banner_shows_once_then_suppressed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("onboarding.json");

        let mut sink = RecordingSink::default();
        assert!(maybe_show_banner(&path, "1.0.0", "http://x/mcp", &mut sink));
        assert!(sink.offered.iter().any(|u| u == "http://x/mcp"));

        let mut sink2 = RecordingSink::default();
        assert!(!maybe_show_banner(
            &path,
            "1.0.0",
            "http://x/mcp",
            &mut sink2
        ));
        assert!(sink2.lines.is_empty());
    }

    #[test]
    fn banner_resurfaces_on_version_change() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("onboarding.json");

        let mut sink = RecordingSink::default();
        assert!(maybe_show_banner(&path, "1.0.0", "http://x/mcp", &mut sink));
        let mut sink2 = RecordingSink::default();
        assert!(maybe_show_banner(
            &path,
            "1.1.0",
            "http://x/mcp",
            &mut sink2
        ));
    }
}
