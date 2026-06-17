// US3 (T013): onboarding banner shows once per version, suppresses on repeat,
// re-surfaces on version change. State path + version are injected; no real
// browser is opened (a recording sink observes the offer).
#![cfg(feature = "server")]

use symforge::cli::onboarding::{self, OnboardingSink, OnboardingState};

/// Records emitted lines and offered URLs; never opens a browser.
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
fn fresh_state_shows_banner_and_records_version() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("onboarding.json");

    let mut sink = RecordingSink::default();
    let shown =
        onboarding::maybe_show_banner(&path, "8.1.0", "http://127.0.0.1:8787/mcp", &mut sink);

    assert!(shown, "fresh state must show the banner");
    assert!(
        sink.offered
            .iter()
            .any(|u| u == "http://127.0.0.1:8787/mcp"),
        "the attach URL must be offered"
    );
    assert!(
        sink.lines.iter().any(|l| l.contains("8.1.0")),
        "the banner must mention the version"
    );

    // State recorded the shown version.
    let state = OnboardingState::load(&path);
    assert_eq!(state.last_shown_version.as_deref(), Some("8.1.0"));
}

#[test]
fn same_version_is_suppressed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("onboarding.json");

    let mut first = RecordingSink::default();
    assert!(onboarding::maybe_show_banner(
        &path,
        "8.1.0",
        "http://x/mcp",
        &mut first
    ));

    let mut second = RecordingSink::default();
    let shown = onboarding::maybe_show_banner(&path, "8.1.0", "http://x/mcp", &mut second);
    assert!(!shown, "same version must not repeat");
    assert!(second.lines.is_empty(), "no banner lines on suppression");
    assert!(second.offered.is_empty(), "no browser offer on suppression");
}

#[test]
fn version_change_resurfaces_banner() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("onboarding.json");

    let mut first = RecordingSink::default();
    assert!(onboarding::maybe_show_banner(
        &path,
        "8.1.0",
        "http://x/mcp",
        &mut first
    ));

    let mut upgraded = RecordingSink::default();
    let shown = onboarding::maybe_show_banner(&path, "8.2.0", "http://x/mcp", &mut upgraded);
    assert!(shown, "a version change must re-surface the banner");

    let state = OnboardingState::load(&path);
    assert_eq!(state.last_shown_version.as_deref(), Some("8.2.0"));
}

#[test]
fn corrupt_state_file_is_treated_as_never_shown() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("onboarding.json");
    std::fs::write(&path, "{ not valid json").unwrap();

    let mut sink = RecordingSink::default();
    let shown = onboarding::maybe_show_banner(&path, "8.1.0", "http://x/mcp", &mut sink);
    assert!(shown, "an unreadable state file falls back to showing once");
}

#[test]
fn state_path_lives_under_symforge_data_dir() {
    let dir = tempfile::tempdir().unwrap();
    let path = onboarding::state_path(dir.path());
    assert!(path.ends_with("onboarding.json"));
    assert!(
        path.to_string_lossy().contains(".symforge"),
        "state path must be under the symforge data dir: {}",
        path.display()
    );
}
