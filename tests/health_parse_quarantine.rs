// Server-only integration test: depends on a `#[cfg(feature = "server")]`
// module (protocol/daemon/cli/sidecar/watcher/analytics). Gating the whole
// file keeps `--no-default-features --features embed --all-targets` compiling.
#![cfg(feature = "server")]

use std::time::{Duration, SystemTime};

use symforge::live_index::store::{
    IndexLoadSource, PublishedIndexState, PublishedIndexStatus, SnapshotVerifyState,
};
use symforge::protocol::format::{
    health_report_compact_from_published_state, health_report_from_published_state,
};
use symforge::watcher::{WatcherInfo, WatcherState};

fn published_state(
    unexpected_partial_files: Vec<String>,
    expected_vendor_partial_files: Vec<String>,
    expected_framework_partial_files: Vec<String>,
    failed_files: Vec<(String, String)>,
) -> PublishedIndexState {
    let partial_parse_count = unexpected_partial_files.len()
        + expected_vendor_partial_files.len()
        + expected_framework_partial_files.len();
    let failed_count = failed_files.len();
    let mut partial_parse_files = unexpected_partial_files.clone();
    partial_parse_files.extend(expected_vendor_partial_files.iter().cloned());
    partial_parse_files.extend(expected_framework_partial_files.iter().cloned());
    partial_parse_files.sort();

    PublishedIndexState {
        generation: 13,
        status: PublishedIndexStatus::Ready,
        degraded_summary: None,
        file_count: partial_parse_count + failed_count + 1,
        parsed_count: 1,
        partial_parse_count,
        unexpected_partial_parse_count: unexpected_partial_files.len(),
        expected_vendor_partial_parse_count: expected_vendor_partial_files.len(),
        expected_framework_partial_parse_count: expected_framework_partial_files.len(),
        failed_count,
        partial_parse_files,
        unexpected_partial_parse_files: unexpected_partial_files,
        expected_vendor_partial_parse_files: expected_vendor_partial_files,
        expected_framework_partial_parse_files: expected_framework_partial_files,
        failed_files,
        symbol_count: 7,
        loaded_at_system: SystemTime::now(),
        load_duration: Duration::from_millis(13),
        load_source: IndexLoadSource::FreshLoad,
        snapshot_verify_state: SnapshotVerifyState::NotNeeded,
        is_empty: false,
        tier_counts: (partial_parse_count + failed_count + 1, 0, 0),
        local_empty_reason: None,
        untracked_indexed: 0,
        indexed_root: None,
    }
}

#[test]
fn health_reports_parse_span_quarantine_registry() {
    let published = published_state(
        vec!["src/broken.rs".to_string()],
        vec!["vendor/tree-sitter-scss/src/parser.c".to_string()],
        vec!["src/app/app.html".to_string()],
        vec![("src/unparseable.rs".to_string(), "lexer panic".to_string())],
    );
    let watcher = WatcherInfo {
        state: WatcherState::Off,
        ..WatcherInfo::default()
    };

    let full = health_report_from_published_state(&published, &watcher, 0);
    assert!(
        full.contains(
            "Parse/span quarantine registry: total=4 unexpected_partial=1 expected_vendor_partial=1 expected_framework_partial=1 failed=1 showing=4 omitted=0"
        ),
        "full health should summarize parse/span quarantine evidence: {full}"
    );
    assert!(
        full.contains("src/broken.rs [unexpected_partial]"),
        "unexpected project partial should be listed as quarantined evidence: {full}"
    );
    assert!(
        full.contains("vendor/tree-sitter-scss/src/parser.c [expected_vendor_partial]"),
        "expected vendor partial should be listed separately: {full}"
    );
    assert!(
        full.contains("src/app/app.html [expected_framework_partial]"),
        "expected framework (Angular) partial should be listed separately: {full}"
    );
    assert!(
        full.contains("src/unparseable.rs [failed] - lexer panic"),
        "failed parse should be listed with its reason: {full}"
    );

    let compact = health_report_compact_from_published_state(&published, &watcher, 0);
    assert!(
        compact.contains(
            "Parse/span quarantine: total=4 unexpected_partial=1 expected_vendor_partial=1 expected_framework_partial=1 failed=1 showing=4 omitted=0"
        ),
        "compact health should retain bounded quarantine counts: {compact}"
    );
}

#[test]
fn clean_health_omits_parse_span_quarantine_registry() {
    let published = published_state(vec![], vec![], vec![], vec![]);
    let watcher = WatcherInfo {
        state: WatcherState::Off,
        ..WatcherInfo::default()
    };

    let full = health_report_from_published_state(&published, &watcher, 0);
    assert!(
        !full.contains("Parse/span quarantine"),
        "clean health should not invent quarantine evidence: {full}"
    );

    let compact = health_report_compact_from_published_state(&published, &watcher, 0);
    assert!(
        !compact.contains("Parse/span quarantine"),
        "clean compact health should not invent quarantine evidence: {compact}"
    );
}

#[test]
fn health_parse_span_quarantine_registry_is_bounded() {
    let unexpected_partial_files: Vec<String> = (0..12)
        .map(|index| format!("src/broken_{index:02}.rs"))
        .collect();
    let published = published_state(unexpected_partial_files, vec![], vec![], vec![]);
    let watcher = WatcherInfo {
        state: WatcherState::Off,
        ..WatcherInfo::default()
    };

    let full = health_report_from_published_state(&published, &watcher, 0);
    assert!(
        full.contains(
            "Parse/span quarantine registry: total=12 unexpected_partial=12 expected_vendor_partial=0 expected_framework_partial=0 failed=0 showing=10 omitted=2"
        ),
        "full health should cap quarantine evidence and report omitted entries: {full}"
    );
    assert!(
        full.contains("src/broken_09.rs [unexpected_partial]"),
        "the tenth bounded entry should be present: {full}"
    );
    assert!(
        !full.contains("src/broken_10.rs [unexpected_partial]"),
        "entries beyond the registry limit should be omitted: {full}"
    );

    let compact = health_report_compact_from_published_state(&published, &watcher, 0);
    assert!(
        compact.contains(
            "Parse/span quarantine: total=12 unexpected_partial=12 expected_vendor_partial=0 expected_framework_partial=0 failed=0 showing=10 omitted=2"
        ),
        "compact health should expose the bounded quarantine count: {compact}"
    );
}

#[test]
fn health_labels_angular_template_partial_as_expected_framework() {
    // SF-004: an Angular `.html` template whose only parse defect is template
    // control-flow (`@if (a > b) {`) lands under the framework bucket, not the
    // repo-owned unexpected bucket.
    let published = published_state(
        vec![],
        vec![],
        vec!["src/app/app.component.html".to_string()],
        vec![],
    );
    let watcher = WatcherInfo {
        state: WatcherState::Off,
        ..WatcherInfo::default()
    };

    let full = health_report_from_published_state(&published, &watcher, 0);
    assert!(
        full.contains(
            "Parse/span quarantine registry: total=1 unexpected_partial=0 expected_vendor_partial=0 expected_framework_partial=1 failed=0 showing=1 omitted=0"
        ),
        "framework partial should be counted in its own bucket: {full}"
    );
    assert!(
        full.contains("src/app/app.component.html [expected_framework_partial]"),
        "Angular template partial should be labeled as a framework limitation: {full}"
    );
    assert!(
        !full.contains("src/app/app.component.html [unexpected_partial]"),
        "Angular template partial must NOT be reported as a repo-owned unexpected partial: {full}"
    );
    // The single framework partial fits in the quarantine registry, so the
    // per-category section is deduped away; the registry carries it with its
    // framework reason (asserted above). The framework path must appear exactly
    // once across the whole report.
    assert!(
        !full.contains("Expected framework partial parse noise"),
        "per-category framework section should be deduped when registry shows all: {full}"
    );
    assert_eq!(
        full.matches("src/app/app.component.html").count(),
        1,
        "framework partial path must appear exactly once (registry only): {full}"
    );

    let compact = health_report_compact_from_published_state(&published, &watcher, 0);
    assert!(
        compact.contains(
            "Parse/span quarantine: total=1 unexpected_partial=0 expected_vendor_partial=0 expected_framework_partial=1 failed=0 showing=1 omitted=0"
        ),
        "compact health should carry the framework bucket count: {compact}"
    );
}
