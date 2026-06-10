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

#[derive(Default)]
struct QuarantineFixture {
    unexpected_partial_files: Vec<String>,
    expected_vendor_partial_files: Vec<String>,
    expected_framework_partial_files: Vec<String>,
    expected_language_partial_files: Vec<String>,
    failed_files: Vec<(String, String)>,
}

fn published_state(fixture: QuarantineFixture) -> PublishedIndexState {
    let QuarantineFixture {
        unexpected_partial_files,
        expected_vendor_partial_files,
        expected_framework_partial_files,
        expected_language_partial_files,
        failed_files,
    } = fixture;

    let partial_parse_count = unexpected_partial_files.len()
        + expected_vendor_partial_files.len()
        + expected_framework_partial_files.len()
        + expected_language_partial_files.len();
    let failed_count = failed_files.len();
    let mut partial_parse_files = unexpected_partial_files.clone();
    partial_parse_files.extend(expected_vendor_partial_files.iter().cloned());
    partial_parse_files.extend(expected_framework_partial_files.iter().cloned());
    partial_parse_files.extend(expected_language_partial_files.iter().cloned());
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
        expected_language_partial_parse_count: expected_language_partial_files.len(),
        failed_count,
        partial_parse_files,
        unexpected_partial_parse_files: unexpected_partial_files,
        expected_vendor_partial_parse_files: expected_vendor_partial_files,
        expected_framework_partial_parse_files: expected_framework_partial_files,
        expected_language_partial_parse_files: expected_language_partial_files,
        failed_files,
        symbol_count: 7,
        loaded_at_system: SystemTime::now(),
        load_duration: Duration::from_millis(13),
        load_source: IndexLoadSource::FreshLoad,
        snapshot_verify_state: SnapshotVerifyState::NotNeeded,
        is_empty: false,
        tier_counts: (partial_parse_count + failed_count + 1, 0, 0),
        local_empty_reason: None,
        indexed_root: None,
        untracked_indexed: 0,
    }
}

#[test]
fn health_reports_parse_span_quarantine_registry() {
    let published = published_state(QuarantineFixture {
        unexpected_partial_files: vec!["src/broken.rs".to_string()],
        expected_vendor_partial_files: vec!["vendor/tree-sitter-scss/src/parser.c".to_string()],
        expected_framework_partial_files: vec!["src/app/app.html".to_string()],
        expected_language_partial_files: vec!["src/types.ts".to_string()],
        failed_files: vec![("src/unparseable.rs".to_string(), "lexer panic".to_string())],
    });
    let watcher = WatcherInfo {
        state: WatcherState::Off,
        ..WatcherInfo::default()
    };

    let full = health_report_from_published_state(&published, &watcher, 0);
    assert!(
        full.contains(
            "Parse/span quarantine registry: total=5 unexpected_partial=1 expected_vendor_partial=1 expected_framework_partial=1 expected_language_partial=1 failed=1 showing=5 omitted=0"
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
        full.contains("src/types.ts [expected_language_partial]"),
        "expected language partial should be listed separately: {full}"
    );
    assert!(
        full.contains("src/unparseable.rs [failed] - lexer panic"),
        "failed parse should be listed with its reason: {full}"
    );

    let compact = health_report_compact_from_published_state(&published, &watcher, 0);
    assert!(
        compact.contains(
            "Parse/span quarantine: total=5 unexpected_partial=1 expected_vendor_partial=1 expected_framework_partial=1 expected_language_partial=1 failed=1 showing=5 omitted=0"
        ),
        "compact health should retain bounded quarantine counts: {compact}"
    );
}

#[test]
fn clean_health_omits_parse_span_quarantine_registry() {
    let published = published_state(QuarantineFixture::default());
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
    let published = published_state(QuarantineFixture {
        unexpected_partial_files,
        ..QuarantineFixture::default()
    });
    let watcher = WatcherInfo {
        state: WatcherState::Off,
        ..WatcherInfo::default()
    };

    let full = health_report_from_published_state(&published, &watcher, 0);
    assert!(
        full.contains(
            "Parse/span quarantine registry: total=12 unexpected_partial=12 expected_vendor_partial=0 expected_framework_partial=0 expected_language_partial=0 failed=0 showing=10 omitted=2"
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
            "Parse/span quarantine: total=12 unexpected_partial=12 expected_vendor_partial=0 expected_framework_partial=0 expected_language_partial=0 failed=0 showing=10 omitted=2"
        ),
        "compact health should expose the bounded quarantine count: {compact}"
    );
}

#[test]
fn health_labels_angular_template_partial_as_expected_framework() {
    // SF-004: an Angular `.html` template whose only parse defect is template
    // control-flow (`@if (a > b) {`) lands under the framework bucket, not the
    // repo-owned unexpected bucket.
    let published = published_state(QuarantineFixture {
        expected_framework_partial_files: vec!["src/app/app.component.html".to_string()],
        ..QuarantineFixture::default()
    });
    let watcher = WatcherInfo {
        state: WatcherState::Off,
        ..WatcherInfo::default()
    };

    let full = health_report_from_published_state(&published, &watcher, 0);
    assert!(
        full.contains(
            "Parse/span quarantine registry: total=1 unexpected_partial=0 expected_vendor_partial=0 expected_framework_partial=1 expected_language_partial=0 failed=0 showing=1 omitted=0"
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
            "Parse/span quarantine: total=1 unexpected_partial=0 expected_vendor_partial=0 expected_framework_partial=1 expected_language_partial=0 failed=0 showing=1 omitted=0"
        ),
        "compact health should carry the framework bucket count: {compact}"
    );
}

#[test]
fn health_labels_typescript_import_type_array_partial_as_expected_language() {
    // SF-003: a TypeScript file whose only parse defect is the known
    // tree-sitter-typescript import-type-array grammar limitation
    // (`import('mod').Member[]`) lands under the language bucket, not the
    // repo-owned unexpected bucket — and is fully accounted for in the registry.
    let published = published_state(QuarantineFixture {
        expected_language_partial_files: vec!["src/app/types.ts".to_string()],
        ..QuarantineFixture::default()
    });
    let watcher = WatcherInfo {
        state: WatcherState::Off,
        ..WatcherInfo::default()
    };

    let full = health_report_from_published_state(&published, &watcher, 0);
    assert!(
        full.contains(
            "Parse/span quarantine registry: total=1 unexpected_partial=0 expected_vendor_partial=0 expected_framework_partial=0 expected_language_partial=1 failed=0 showing=1 omitted=0"
        ),
        "language partial should be counted in its own bucket: {full}"
    );
    assert!(
        full.contains("src/app/types.ts [expected_language_partial]"),
        "TypeScript import-type-array partial should be labeled as a language limitation: {full}"
    );
    assert!(
        !full.contains("src/app/types.ts [unexpected_partial]"),
        "language partial must NOT be reported as a repo-owned unexpected partial: {full}"
    );
    // Item 6a (quarantine dedup): when the language partial already fits in the
    // registry, the per-category "Expected language partial parse noise" section
    // is suppressed so the path is listed exactly once (registry only) — never
    // duplicated by a redundant detail section.
    assert_eq!(
        full.matches("src/app/types.ts").count(),
        1,
        "language partial path must appear exactly once (registry only), not duplicated by a per-category section: {full}"
    );
    assert!(
        !full.contains("Expected language partial parse noise (not shown above)"),
        "no overflow section should render when the registry already shows every language partial: {full}"
    );

    let compact = health_report_compact_from_published_state(&published, &watcher, 0);
    assert!(
        compact.contains(
            "Parse/span quarantine: total=1 unexpected_partial=0 expected_vendor_partial=0 expected_framework_partial=0 expected_language_partial=1 failed=0 showing=1 omitted=0"
        ),
        "compact health should carry the language bucket count: {compact}"
    );
}

#[test]
fn health_registry_total_accounts_for_every_partial_including_excused() {
    // Regression for the reported testpilot mismatch: the header counted 2
    // partial files but the registry summed to 1 because the SF-003-excused
    // TypeScript file landed in NO bucket. The registry total MUST equal the
    // sum of partial + failed, so every partial is visible somewhere.
    let published = published_state(QuarantineFixture {
        unexpected_partial_files: vec!["frontend/src/app/app.html".to_string()],
        expected_language_partial_files: vec!["frontend/src/app/state.ts".to_string()],
        ..QuarantineFixture::default()
    });

    let registry_total = published.unexpected_partial_parse_count
        + published.expected_vendor_partial_parse_count
        + published.expected_framework_partial_parse_count
        + published.expected_language_partial_parse_count
        + published.failed_count;
    assert_eq!(
        registry_total,
        published.partial_parse_count + published.failed_count,
        "registry total must account for every partial parse"
    );

    let watcher = WatcherInfo {
        state: WatcherState::Off,
        ..WatcherInfo::default()
    };
    let full = health_report_from_published_state(&published, &watcher, 0);
    // The header reports 2 partials; the registry total must also be 2.
    assert!(
        full.contains("(1 parsed, 2 partial, 0 failed)"),
        "header should report both partial files: {full}"
    );
    assert!(
        full.contains(
            "Parse/span quarantine registry: total=2 unexpected_partial=1 expected_vendor_partial=0 expected_framework_partial=0 expected_language_partial=1 failed=0 showing=2 omitted=0"
        ),
        "registry total must equal the header partial count — no invisible partials: {full}"
    );
}
