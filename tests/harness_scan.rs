// US1 (T005): HarnessRegistry::scan reports absent / present-current /
// present-stale / not-installed / malformed against fixture configs.
//
// Server-only: the CLI surface is behind `#[cfg(feature = "server")]`.
#![cfg(feature = "server")]

use std::path::PathBuf;

use symforge::cli::harness::{
    AttachEntry, HarnessFormat, HarnessId, HarnessRegistry, HarnessState, HarnessTarget,
};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/harness");

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(FIXTURES).join(name)
}

fn desired() -> AttachEntry {
    AttachEntry::new(
        "http://127.0.0.1:8787/mcp",
        Some("sf_current_key".to_string()),
    )
}

fn json_target(id: HarnessId, path: PathBuf) -> HarnessTarget {
    HarnessTarget {
        id,
        config_path: path,
        format: HarnessFormat::Json,
    }
}

fn scan_one(target: HarnessTarget) -> HarnessState {
    let reg = HarnessRegistry::from_targets(vec![target]);
    reg.scan(&desired()).into_iter().next().unwrap().state
}

#[test]
fn populated_config_without_symforge_is_absent() {
    let state = scan_one(json_target(
        HarnessId::ClaudeCode,
        fixture("claude_populated.json"),
    ));
    assert_eq!(state, HarnessState::Absent);
}

#[test]
fn empty_config_is_absent() {
    let state = scan_one(json_target(
        HarnessId::ClaudeCode,
        fixture("claude_empty.json"),
    ));
    assert_eq!(state, HarnessState::Absent);
}

#[test]
fn stale_symforge_entry_is_present_stale() {
    // The fixture has url=http://127.0.0.1:9999/mcp + Bearer sf_old_stale_key,
    // which differs from `desired()`.
    let state = scan_one(json_target(
        HarnessId::ClaudeCode,
        fixture("claude_stale_symforge.json"),
    ));
    assert_eq!(state, HarnessState::PresentStale);
}

#[test]
fn matching_symforge_entry_is_present_current() {
    // Write a temp config whose entry matches `desired()` exactly.
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("claude.json");
    std::fs::write(
        &cfg,
        r#"{"mcpServers":{"symforge":{"type":"http","url":"http://127.0.0.1:8787/mcp","headers":{"Authorization":"Bearer sf_current_key"}}}}"#,
    )
    .unwrap();
    let state = scan_one(json_target(HarnessId::ClaudeCode, cfg));
    assert_eq!(state, HarnessState::PresentCurrent);
}

#[test]
fn missing_config_in_missing_dir_is_not_installed() {
    let dir = tempfile::tempdir().unwrap();
    // A path under a non-existent subdirectory: parent does not exist.
    let cfg = dir.path().join("nope").join("config.json");
    let state = scan_one(json_target(HarnessId::Cursor, cfg));
    assert_eq!(state, HarnessState::NotInstalled);
}

#[test]
fn missing_config_with_existing_dir_is_absent() {
    let dir = tempfile::tempdir().unwrap();
    // Parent (the temp dir) exists, file does not => client installed, no entry.
    let cfg = dir.path().join("config.json");
    let state = scan_one(json_target(HarnessId::Cursor, cfg));
    assert_eq!(state, HarnessState::Absent);
}

#[test]
fn malformed_config_is_reported_not_parsed() {
    let state = scan_one(json_target(
        HarnessId::ClaudeCode,
        fixture("malformed.json"),
    ));
    assert!(matches!(state, HarnessState::Malformed(_)));
}

#[test]
fn bom_encoded_config_parses_as_absent() {
    // The BOM fixture is valid JSON with a leading UTF-8 BOM and no symforge
    // entry; BOM-safe read => Absent (not Malformed).
    let state = scan_one(json_target(HarnessId::ClaudeCode, fixture("bom_utf8.json")));
    assert_eq!(state, HarnessState::Absent);
}

#[test]
fn codex_toml_without_symforge_is_absent() {
    let target = HarnessTarget {
        id: HarnessId::Codex,
        config_path: fixture("codex.toml"),
        format: HarnessFormat::Toml,
    };
    assert_eq!(scan_one(target), HarnessState::Absent);
}

#[test]
fn codex_toml_with_matching_symforge_is_present_current() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("config.toml");
    std::fs::write(
        &cfg,
        "[mcp_servers.symforge]\nurl = \"http://127.0.0.1:8787/mcp\"\nbearer_token = \"sf_current_key\"\n",
    )
    .unwrap();
    let target = HarnessTarget {
        id: HarnessId::Codex,
        config_path: cfg,
        format: HarnessFormat::Toml,
    };
    assert_eq!(scan_one(target), HarnessState::PresentCurrent);
}

#[test]
fn known_registry_lists_all_six_clients() {
    let dir = tempfile::tempdir().unwrap();
    let reg = HarnessRegistry::known_with(dir.path(), dir.path());
    let ids: Vec<HarnessId> = reg.targets().iter().map(|t| t.id).collect();
    assert!(ids.contains(&HarnessId::ClaudeCode));
    assert!(ids.contains(&HarnessId::ClaudeDesktop));
    assert!(ids.contains(&HarnessId::Codex));
    assert!(ids.contains(&HarnessId::Gemini));
    assert!(ids.contains(&HarnessId::KiloCode));
    assert!(ids.contains(&HarnessId::Cursor));
    assert_eq!(ids.len(), 6);
}
