//! Transport-independent health rendering shared by the MCP `health` /
//! `health_compact` handlers and the embedded host health projection.
//!
//! Every function here reads already-published index state (plus the caller's
//! watcher snapshot and placement) and returns text. No server dependencies,
//! so both surfaces render byte-identical sections from the same inputs.

use crate::live_index::query::{
    EXPECTED_FRAMEWORK_PARTIAL_PARSE_REASON, EXPECTED_GENERATED_PARTIAL_PARSE_REASON,
    EXPECTED_LANGUAGE_PARTIAL_PARSE_REASON, EXPECTED_TEMPLATE_DSL_PARTIAL_PARSE_REASON,
    EXPECTED_TEST_FIXTURE_PARTIAL_PARSE_REASON, EXPECTED_VENDOR_PARTIAL_PARSE_REASON,
};
use crate::live_index::{HealthStats, IndexLoadSource, PublishedIndexState, SnapshotVerifyState};

pub(crate) const PARSE_QUARANTINE_ENTRY_LIMIT: usize = 10;

/// Upper bound on the quarantine display window the `health` `quarantine_limit`
/// parameter may request (SF-STRESS-010). Caps token cost while still letting an
/// agent page the full list in a few calls rather than thousands of per-file
/// `get_file_context` calls.
pub const PARSE_QUARANTINE_MAX_LIMIT: usize = 1000;

/// Display window into the ranked quarantine registry (SF-STRESS-010).
/// `None` everywhere preserves the historical default (offset 0, limit 10).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QuarantineWindow {
    pub offset: usize,
    pub limit: usize,
}

impl Default for QuarantineWindow {
    /// The historical default registry window: offset 0, the legacy 10-entry cap.
    fn default() -> Self {
        Self {
            offset: 0,
            limit: PARSE_QUARANTINE_ENTRY_LIMIT,
        }
    }
}

impl QuarantineWindow {
    /// Build a window from optional caller-supplied paging args, clamping the
    /// limit to [`PARSE_QUARANTINE_MAX_LIMIT`]. `None` limit means the default.
    pub fn from_args(offset: Option<usize>, limit: Option<usize>) -> Self {
        Self {
            offset: offset.unwrap_or(0),
            limit: limit
                .unwrap_or(PARSE_QUARANTINE_ENTRY_LIMIT)
                .clamp(1, PARSE_QUARANTINE_MAX_LIMIT),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ParseQuarantineKind {
    UnexpectedPartial,
    ExpectedVendorPartial,
    ExpectedGeneratedPartial,
    ExpectedTestFixturePartial,
    ExpectedTemplateDslPartial,
    ExpectedFrameworkPartial,
    ExpectedLanguagePartial,
    Failed,
}

impl ParseQuarantineKind {
    fn label(self) -> &'static str {
        match self {
            Self::UnexpectedPartial => "unexpected_partial",
            Self::ExpectedVendorPartial => "expected_vendor_partial",
            Self::ExpectedGeneratedPartial => "expected_generated_partial",
            Self::ExpectedTestFixturePartial => "expected_test_fixture_partial",
            Self::ExpectedTemplateDslPartial => "expected_template_dsl_partial",
            Self::ExpectedFrameworkPartial => "expected_framework_partial",
            Self::ExpectedLanguagePartial => "expected_language_partial",
            Self::Failed => "failed",
        }
    }

    /// Real-source-first ranking key (SF-STRESS-010): unexpected repo-owned
    /// partials and hard failures rank BEFORE heuristic-noise buckets, so a
    /// `deps/`-flood cannot fill the default registry slots and hide genuine
    /// `src/` losses. Lower sorts first.
    fn rank(self) -> u8 {
        match self {
            Self::Failed => 0,
            Self::UnexpectedPartial => 1,
            // Sound (proven) limitation buckets — still repo-relevant.
            Self::ExpectedFrameworkPartial => 2,
            Self::ExpectedLanguagePartial => 3,
            // Heuristic noise buckets — least likely to need attention.
            Self::ExpectedTemplateDslPartial => 4,
            Self::ExpectedTestFixturePartial => 5,
            Self::ExpectedGeneratedPartial => 6,
            Self::ExpectedVendorPartial => 7,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ParseQuarantineEntry {
    path: String,
    kind: ParseQuarantineKind,
    reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ParseQuarantineSummary {
    total_count: usize,
    unexpected_partial_count: usize,
    expected_vendor_partial_count: usize,
    expected_generated_partial_count: usize,
    expected_test_fixture_partial_count: usize,
    expected_template_dsl_partial_count: usize,
    expected_framework_partial_count: usize,
    expected_language_partial_count: usize,
    failed_count: usize,
    /// Every collected entry, ranked real-source-first (see
    /// [`ParseQuarantineKind::rank`]). The display layer windows this with
    /// [`Self::display_limit`]; the full vector is what makes the list
    /// retrievable (SF-STRESS-010) once the data layer stops truncating at
    /// capture time.
    entries: Vec<ParseQuarantineEntry>,
    /// How many ranked entries the registry renders. Defaults to
    /// [`PARSE_QUARANTINE_ENTRY_LIMIT`]; the `health` `quarantine_limit`
    /// parameter raises it so the full list is retrievable (SF-STRESS-010).
    display_limit: usize,
    /// Offset into the ranked entries for the rendered window (paging).
    display_offset: usize,
}

impl ParseQuarantineSummary {
    fn from_stats(stats: &HealthStats) -> Self {
        let mut summary = Self::new(
            stats.unexpected_partial_parse_count,
            stats.expected_vendor_partial_parse_count,
            stats.expected_generated_partial_parse_count,
            stats.expected_test_fixture_partial_parse_count,
            stats.expected_template_dsl_partial_parse_count,
            stats.expected_framework_partial_parse_count,
            stats.expected_language_partial_parse_count,
            stats.failed_count,
        );
        summary.push_partials(
            &stats.unexpected_partial_parse_files,
            &stats.expected_vendor_partial_parse_files,
            &stats.expected_generated_partial_parse_files,
            &stats.expected_test_fixture_partial_parse_files,
            &stats.expected_template_dsl_partial_parse_files,
            &stats.expected_framework_partial_parse_files,
            &stats.expected_language_partial_parse_files,
            &stats.failed_files,
        );
        summary
    }

    fn from_published(published: &PublishedIndexState) -> Self {
        let mut summary = Self::new(
            published.unexpected_partial_parse_count,
            published.expected_vendor_partial_parse_count,
            published.expected_generated_partial_parse_count,
            published.expected_test_fixture_partial_parse_count,
            published.expected_template_dsl_partial_parse_count,
            published.expected_framework_partial_parse_count,
            published.expected_language_partial_parse_count,
            published.failed_count,
        );
        summary.push_partials(
            &published.unexpected_partial_parse_files,
            &published.expected_vendor_partial_parse_files,
            &published.expected_generated_partial_parse_files,
            &published.expected_test_fixture_partial_parse_files,
            &published.expected_template_dsl_partial_parse_files,
            &published.expected_framework_partial_parse_files,
            &published.expected_language_partial_parse_files,
            &published.failed_files,
        );
        summary
    }

    /// Collect every available path into ranked entries. The data layer may have
    /// already bounded each list, but this consumes whatever is present in full
    /// (no per-call `.take()`), so the display window — not collection — controls
    /// truncation (SF-STRESS-010).
    #[allow(clippy::too_many_arguments)]
    fn push_partials(
        &mut self,
        unexpected: &[String],
        vendor: &[String],
        generated: &[String],
        test_fixture: &[String],
        template_dsl: &[String],
        framework: &[String],
        language: &[String],
        failed: &[(String, String)],
    ) {
        for path in unexpected {
            self.push(
                path,
                ParseQuarantineKind::UnexpectedPartial,
                "repo-owned partial parse; best-effort symbols may be incomplete",
            );
        }
        for path in vendor {
            self.push(
                path,
                ParseQuarantineKind::ExpectedVendorPartial,
                EXPECTED_VENDOR_PARTIAL_PARSE_REASON,
            );
        }
        for path in generated {
            self.push(
                path,
                ParseQuarantineKind::ExpectedGeneratedPartial,
                EXPECTED_GENERATED_PARTIAL_PARSE_REASON,
            );
        }
        for path in test_fixture {
            self.push(
                path,
                ParseQuarantineKind::ExpectedTestFixturePartial,
                EXPECTED_TEST_FIXTURE_PARTIAL_PARSE_REASON,
            );
        }
        for path in template_dsl {
            self.push(
                path,
                ParseQuarantineKind::ExpectedTemplateDslPartial,
                EXPECTED_TEMPLATE_DSL_PARTIAL_PARSE_REASON,
            );
        }
        for path in framework {
            self.push(
                path,
                ParseQuarantineKind::ExpectedFrameworkPartial,
                EXPECTED_FRAMEWORK_PARTIAL_PARSE_REASON,
            );
        }
        for path in language {
            self.push(
                path,
                ParseQuarantineKind::ExpectedLanguagePartial,
                EXPECTED_LANGUAGE_PARTIAL_PARSE_REASON,
            );
        }
        for (path, error) in failed {
            self.push(path, ParseQuarantineKind::Failed, error);
        }
        // Real-source-first ordering so a vendored/fixture flood cannot evict
        // genuine repo-owned losses from the default display window
        // (SF-STRESS-010). Stable by (rank, original insertion order within the
        // already-sorted category lists), preserving alphabetical paths per kind.
        self.entries
            .sort_by_key(|entry| (entry.kind.rank(), entry.path.clone()));
    }

    #[allow(clippy::too_many_arguments)]
    fn new(
        unexpected_partial_count: usize,
        expected_vendor_partial_count: usize,
        expected_generated_partial_count: usize,
        expected_test_fixture_partial_count: usize,
        expected_template_dsl_partial_count: usize,
        expected_framework_partial_count: usize,
        expected_language_partial_count: usize,
        failed_count: usize,
    ) -> Self {
        Self {
            total_count: unexpected_partial_count
                + expected_vendor_partial_count
                + expected_generated_partial_count
                + expected_test_fixture_partial_count
                + expected_template_dsl_partial_count
                + expected_framework_partial_count
                + expected_language_partial_count
                + failed_count,
            unexpected_partial_count,
            expected_vendor_partial_count,
            expected_generated_partial_count,
            expected_test_fixture_partial_count,
            expected_template_dsl_partial_count,
            expected_framework_partial_count,
            expected_language_partial_count,
            failed_count,
            entries: Vec::new(),
            display_limit: PARSE_QUARANTINE_ENTRY_LIMIT,
            display_offset: 0,
        }
    }

    /// Override the display window (SF-STRESS-010 retrieval surface). `limit` is
    /// clamped to at least 1; `offset` pages into the ranked entries.
    fn with_window(mut self, offset: usize, limit: usize) -> Self {
        self.display_offset = offset;
        self.display_limit = limit.max(1);
        self
    }

    /// The ranked entries actually rendered in the current display window.
    fn windowed_entries(&self) -> &[ParseQuarantineEntry] {
        let start = self.display_offset.min(self.entries.len());
        let end = start
            .saturating_add(self.display_limit)
            .min(self.entries.len());
        &self.entries[start..end]
    }

    /// Collect ALL entries (no display cap). The display window is applied
    /// separately via [`Self::windowed_entries`].
    fn push(&mut self, path: &str, kind: ParseQuarantineKind, reason: &str) {
        self.entries.push(ParseQuarantineEntry {
            path: path.to_string(),
            kind,
            reason: reason.to_string(),
        });
    }

    fn is_empty(&self) -> bool {
        self.total_count == 0
    }

    /// Count metadata fields shared by the full and compact headers. Always
    /// reports the TRUE category totals (`total_count` etc. come from the
    /// uncapped health counts), while `showing`/`omitted` reflect the current
    /// display window so the header never overstates what is on screen.
    fn header_counts(&self) -> String {
        let shown = self.windowed_entries().len();
        // Entries beyond the current window, relative to the true total — covers
        // BOTH display windowing and any data-layer bound on `entries`.
        let omitted = self
            .total_count
            .saturating_sub(self.display_offset.saturating_add(shown));
        // `total`, `unexpected_partial`, `failed` and the window fields are
        // always reported: they are what a caller acts on, and a zero in any of
        // them is itself the answer.
        //
        // The expected_* categories are not. They are almost always zero, and
        // this header spent six of its twelve fields saying so — on this repo
        // that is roughly half the line conveying nothing. Emit only the
        // categories that actually have entries; an absent category means zero,
        // exactly as it does for the summary line above.
        let mut parts = vec![
            format!("total={}", self.total_count),
            format!("unexpected_partial={}", self.unexpected_partial_count),
        ];
        for (label, count) in [
            (
                "expected_vendor_partial",
                self.expected_vendor_partial_count,
            ),
            (
                "expected_generated_partial",
                self.expected_generated_partial_count,
            ),
            (
                "expected_test_fixture_partial",
                self.expected_test_fixture_partial_count,
            ),
            (
                "expected_template_dsl_partial",
                self.expected_template_dsl_partial_count,
            ),
            (
                "expected_framework_partial",
                self.expected_framework_partial_count,
            ),
            (
                "expected_language_partial",
                self.expected_language_partial_count,
            ),
        ] {
            if count > 0 {
                parts.push(format!("{label}={count}"));
            }
        }
        parts.push(format!("failed={}", self.failed_count));
        parts.push(format!("showing={shown}"));
        parts.push(format!("offset={}", self.display_offset));
        parts.push(format!("omitted={omitted}"));
        parts.join(" ")
    }

    /// Paths in the CURRENT display window. Used by the per-category health
    /// sections to avoid re-listing entries the registry already displays (they
    /// only render genuinely-omitted overflow paths).
    fn shown_paths(&self) -> std::collections::HashSet<&str> {
        self.windowed_entries()
            .iter()
            .map(|e| e.path.as_str())
            .collect()
    }

    fn full_section(&self) -> Option<String> {
        if self.is_empty() {
            return None;
        }

        let mut section = format!("Parse/span quarantine registry: {}", self.header_counts());
        for (index, entry) in self.windowed_entries().iter().enumerate() {
            section.push_str(&format!(
                "\n  {}. {} [{}] - {}",
                self.display_offset + index + 1,
                entry.path,
                entry.kind.label(),
                entry.reason
            ));
        }
        // SF-STRESS-010: when the registry does not show the full list, point the
        // caller at the retrieval surface instead of leaving them to thousands of
        // per-file calls.
        if self.total_count.saturating_sub(
            self.display_offset
                .saturating_add(self.windowed_entries().len()),
        ) > 0
        {
            section.push_str(
                "\n  (use health with quarantine_limit/quarantine_offset to page the full list)",
            );
        }
        Some(section)
    }

    fn compact_line(&self) -> Option<String> {
        if self.is_empty() {
            return None;
        }

        Some(format!("Parse/span quarantine: {}", self.header_counts()))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeMode {
    LocalProcess,
    DaemonReusedSession,
    DaemonDegradedLocalFallback,
    /// A library host serving an embedded source: no daemon, no MCP session.
    Embedded,
}

impl RuntimeMode {
    fn label(self) -> &'static str {
        match self {
            Self::LocalProcess => "local_process",
            Self::DaemonReusedSession => "daemon_reused_session",
            Self::DaemonDegradedLocalFallback => "daemon_degraded_local_fallback",
            Self::Embedded => "embedded",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeStatus {
    pub mode: RuntimeMode,
    pub project_root: Option<String>,
    pub project_id: String,
    pub session_id: String,
    pub index_generation: u64,
    pub project_generation: u64,
    pub reset_project_generation: Option<u64>,
    pub load_source: IndexLoadSource,
}

pub(crate) fn index_load_source_label(load_source: IndexLoadSource) -> &'static str {
    load_source.label()
}

pub(crate) fn index_state_label(status: &RuntimeStatus) -> &'static str {
    if current_project_was_reset(status) {
        return "index_folder_reset";
    }

    match status.load_source {
        IndexLoadSource::EmptyBootstrap => "empty_bootstrap",
        IndexLoadSource::FreshLoad => "fresh_process",
        IndexLoadSource::SnapshotRestore => "snapshot_loaded_reused",
    }
}

pub(crate) fn current_project_was_reset(status: &RuntimeStatus) -> bool {
    status.project_generation > 0
        && status.reset_project_generation == Some(status.project_generation)
}

pub(crate) fn reset_state_label(status: &RuntimeStatus) -> String {
    match status.reset_project_generation {
        Some(generation) if generation == status.project_generation => {
            format!("current_project:p{generation}")
        }
        Some(generation) => format!("previous_project:p{generation}"),
        None => "none".to_string(),
    }
}

pub(crate) fn snapshot_verify_state_label(state: &SnapshotVerifyState) -> &'static str {
    match state {
        SnapshotVerifyState::NotNeeded => "not_needed",
        SnapshotVerifyState::Pending => "pending",
        SnapshotVerifyState::Running(_) => "running",
        SnapshotVerifyState::Completed(_) => "completed",
        SnapshotVerifyState::Failed(_) => "failed",
    }
}

pub(crate) fn append_snapshot_verify_mismatch_summary(
    line: &mut String,
    state: &SnapshotVerifyState,
    path_limit: usize,
) {
    if let SnapshotVerifyState::Running(progress) = state {
        line.push(' ');
        line.push_str(&progress.describe());
    }
    if let SnapshotVerifyState::Completed(report) | SnapshotVerifyState::Failed(report) = state {
        line.push_str(&format!(" mismatches={}", report.mismatch_count));
        if report.mismatch_count > 0 {
            let shown = report.mismatched_paths.len().min(path_limit);
            let omitted = report.mismatch_count.saturating_sub(shown);
            line.push_str(&format!(" showing={shown} omitted={omitted}"));
        }
        if let Some(reason) = &report.reason {
            line.push_str(&format!(" reason={}", quoted_status_value(reason)));
        }
        if let Some(error) = &report.discovery_error {
            line.push_str(&format!(" discovery_error={}", quoted_status_value(error)));
        }
        if report.mismatch_count > 0 {
            let shown = report.mismatched_paths.len().min(path_limit);
            if shown > 0 {
                let paths = report
                    .mismatched_paths
                    .iter()
                    .take(shown)
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .join(", ");
                line.push_str(" paths=");
                line.push_str(&paths);
            }
        }
        if !report.unverified.is_empty() {
            let shown = report.unverified.len().min(path_limit);
            let listed = report
                .unverified
                .iter()
                .take(shown)
                .map(|(path, reason)| format!("{path}: {reason}"))
                .collect::<Vec<_>>()
                .join("; ");
            line.push_str(&format!(
                " unverified_since_restore={} unverified_omitted={} unverified={}",
                report.unverified.len(),
                report.unverified.len() - shown,
                quoted_status_value(&listed)
            ));
        }
    }
}

/// A double-quoted `key=value` value: backslashes and quotes escaped, and
/// control characters (newlines in an error message) flattened to spaces, so
/// free text can never end the value or the line early.
pub(crate) fn quoted_status_value(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => quoted.push_str("\\\\"),
            '"' => quoted.push_str("\\\""),
            ch if ch.is_control() => quoted.push(' '),
            ch => quoted.push(ch),
        }
    }
    quoted.push('"');
    quoted
}

pub(crate) fn snapshot_verify_health_line(published: &PublishedIndexState) -> Option<String> {
    if published.load_source != IndexLoadSource::SnapshotRestore
        && matches!(
            &published.snapshot_verify_state,
            SnapshotVerifyState::NotNeeded
        )
    {
        return None;
    }

    let mut line = format!(
        "Snapshot verify: load_source={} state={}",
        index_load_source_label(published.load_source),
        snapshot_verify_state_label(&published.snapshot_verify_state)
    );
    append_snapshot_verify_mismatch_summary(&mut line, &published.snapshot_verify_state, 10);
    Some(line)
}

pub(crate) fn snapshot_verify_compact_line(published: &PublishedIndexState) -> Option<String> {
    snapshot_verify_status_line(published.load_source, &published.snapshot_verify_state)
}

/// Compact snapshot-verify line shared by `health_compact` and `status`;
/// `None` when no snapshot was restored.
pub(crate) fn snapshot_verify_status_line(
    load_source: IndexLoadSource,
    state: &SnapshotVerifyState,
) -> Option<String> {
    if load_source != IndexLoadSource::SnapshotRestore
        && matches!(state, SnapshotVerifyState::NotNeeded)
    {
        return None;
    }

    let mut line = format!(
        "Snapshot: load_source={} verify={}",
        index_load_source_label(load_source),
        snapshot_verify_state_label(state)
    );
    append_snapshot_verify_mismatch_summary(&mut line, state, 3);
    Some(line)
}

pub(crate) fn index_identity(status: &RuntimeStatus) -> String {
    let suffix: String = status
        .project_id
        .strip_prefix("project-")
        .unwrap_or(&status.project_id)
        .chars()
        .take(12)
        .collect();
    format!(
        "index-{suffix}-p{}-g{}",
        status.project_generation, status.index_generation
    )
}

/// Version of the binary actually serving this runtime status line.
///
/// Included in EVERY mode so a `local_process` front-end (which carries no
/// `daemon_version`) still reports which binary is running, removing the
/// version-provenance ambiguity that existed when only daemon mode surfaced a
/// version. Consistent with the daemon's `daemon_version`, which is sourced
/// from the same `CARGO_PKG_VERSION`.
pub(crate) const RUNTIME_BINARY_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn format_runtime_status(status: &RuntimeStatus) -> String {
    format!(
        "Runtime: mode={} | runtime_state={} | version={} | project_root={} | project_id={} | session_id={} | index_id={} | index_generation={} | project_generation={} | load_source={} | reset_state={} | index_state={}",
        status.mode.label(),
        status.mode.label(),
        RUNTIME_BINARY_VERSION,
        status.project_root.as_deref().unwrap_or("<unknown>"),
        status.project_id,
        status.session_id,
        index_identity(status),
        status.index_generation,
        status.project_generation,
        index_load_source_label(status.load_source),
        reset_state_label(status),
        index_state_label(status),
    )
}

pub fn format_runtime_status_compact(status: &RuntimeStatus) -> String {
    format!(
        "Runtime: mode={} | version={} | project_root={} | project_id={} | session_id={} | index_id={} | index_generation={} | project_generation={} | load_source={} | reset_state={} | index_state={}",
        status.mode.label(),
        RUNTIME_BINARY_VERSION,
        status.project_root.as_deref().unwrap_or("<unknown>"),
        status.project_id,
        status.session_id,
        index_identity(status),
        status.index_generation,
        status.project_generation,
        index_load_source_label(status.load_source),
        reset_state_label(status),
        index_state_label(status),
    )
}

pub fn health_report_from_published_state(
    published: &PublishedIndexState,
    watcher: &crate::watcher_state::WatcherInfo,
    rejected_stale_mutations: u64,
) -> String {
    health_report_from_published_state_windowed(
        published,
        watcher,
        rejected_stale_mutations,
        QuarantineWindow::from_args(None, None),
    )
}

/// Like [`health_report_from_published_state`] but renders the quarantine
/// registry through the given display window (SF-STRESS-010 retrieval surface).
pub fn health_report_from_published_state_windowed(
    published: &PublishedIndexState,
    watcher: &crate::watcher_state::WatcherInfo,
    rejected_stale_mutations: u64,
    quarantine_window: QuarantineWindow,
) -> String {
    let mut stats = HealthStats {
        file_count: published.file_count,
        symbol_count: published.symbol_count,
        parsed_count: published.parsed_count,
        partial_parse_count: published.partial_parse_count,
        unexpected_partial_parse_count: published.unexpected_partial_parse_count,
        expected_vendor_partial_parse_count: published.expected_vendor_partial_parse_count,
        expected_generated_partial_parse_count: published.expected_generated_partial_parse_count,
        expected_test_fixture_partial_parse_count: published
            .expected_test_fixture_partial_parse_count,
        expected_template_dsl_partial_parse_count: published
            .expected_template_dsl_partial_parse_count,
        expected_framework_partial_parse_count: published.expected_framework_partial_parse_count,
        expected_language_partial_parse_count: published.expected_language_partial_parse_count,
        failed_count: published.failed_count,
        load_duration: published.load_duration,
        watcher_state: watcher.state.clone(),
        events_processed: watcher.events_processed,
        last_event_at: watcher.last_event_at,
        debounce_window_ms: watcher.debounce_window_ms,
        overflow_count: watcher.overflow_count,
        last_overflow_at: watcher.last_overflow_at,
        stale_files_found: watcher.stale_files_found,
        last_reconcile_at: watcher.last_reconcile_at,
        partial_parse_files: published.partial_parse_files.clone(),
        unexpected_partial_parse_files: published.unexpected_partial_parse_files.clone(),
        expected_vendor_partial_parse_files: published.expected_vendor_partial_parse_files.clone(),
        expected_generated_partial_parse_files: published
            .expected_generated_partial_parse_files
            .clone(),
        expected_test_fixture_partial_parse_files: published
            .expected_test_fixture_partial_parse_files
            .clone(),
        expected_template_dsl_partial_parse_files: published
            .expected_template_dsl_partial_parse_files
            .clone(),
        expected_framework_partial_parse_files: published
            .expected_framework_partial_parse_files
            .clone(),
        expected_language_partial_parse_files: published
            .expected_language_partial_parse_files
            .clone(),
        failed_files: published.failed_files.clone(),
        tier_counts: published.tier_counts,
        local_empty_reason: published.local_empty_reason.clone(),
        untracked_indexed: published.untracked_indexed,
    };
    // Preserve the existing formatter shape by reusing HealthStats.
    if matches!(stats.watcher_state, crate::watcher_state::WatcherState::Off) {
        stats.events_processed = 0;
        stats.last_event_at = None;
    }
    let mut report = health_report_from_stats_windowed(
        published.status_label(),
        &stats,
        rejected_stale_mutations,
        quarantine_window,
    );
    if let Some(line) = snapshot_verify_health_line(published) {
        report.push('\n');
        report.push_str(&line);
    }
    report
}

pub fn health_report_compact_from_published_state(
    published: &PublishedIndexState,
    watcher: &crate::watcher_state::WatcherInfo,
    rejected_stale_mutations: u64,
) -> String {
    use crate::watcher_state::WatcherState;

    let watcher_label = if watcher.is_local_fallback() {
        "local-fallback (no watcher attached)".to_string()
    } else {
        match &watcher.state {
            WatcherState::Active
                if watcher.events_processed == 0
                    && watcher.last_event_at.is_none()
                    && watcher.overflow_count == 0
                    && watcher.stale_files_found == 0 =>
            {
                "active/idle".to_string()
            }
            WatcherState::Active => format!(
                "active (events: {}, overflows: {}, repairs: {})",
                watcher.events_processed, watcher.overflow_count, watcher.stale_files_found
            ),
            WatcherState::Starting => "starting (registering filesystem watch)".to_string(),
            WatcherState::Degraded => format!(
                "degraded (events: {}, overflows: {}, repairs: {})",
                watcher.events_processed, watcher.overflow_count, watcher.stale_files_found
            ),
            WatcherState::Off => "off".to_string(),
        }
    };
    let (tier1, tier2, tier3) = published.tier_counts;
    let mut output = format!(
        "Status: {} | Files: {} indexed ({} parsed, {} partial, {} failed) | Symbols: {} | Loaded: {}ms\nWatcher: {} | Stale-mutation rejections: {} | Admission tiers: {}/{}/{} (indexed/metadata/skipped)",
        published.status_label(),
        published.file_count,
        published.parsed_count,
        published.partial_parse_count,
        published.failed_count,
        published.symbol_count,
        published.load_duration.as_millis(),
        watcher_label,
        rejected_stale_mutations,
        tier1,
        tier2,
        tier3,
    );

    if published.partial_parse_count > 0 || published.failed_count > 0 {
        output.push_str(&format!(
            "\nParse issues: {} unexpected partial, {} expected vendor partial, {} failed; use full health for path lists",
            published.unexpected_partial_parse_count,
            published.expected_vendor_partial_parse_count,
            published.failed_count
        ));
    }

    if let Some(line) = ParseQuarantineSummary::from_published(published).compact_line() {
        output.push('\n');
        output.push_str(&line);
    }

    if let Some(line) = snapshot_verify_compact_line(published) {
        output.push('\n');
        output.push_str(&line);
    }

    if published.file_count == 0 {
        output.push_str(
            "\n⚠ Empty index — run index_folder(path=\"<your-repo-root>\") to bind an index",
        );
    }

    output
}

// ── Feature 020 repository-knowledge health (M-001) ──────────────────────────
//
// Surfaces already-published Feature 020 evidence (manifest, dispositions,
// source set, bridge, authority hygiene) plus source-binding/runtime state
// (binding, authorization, placement, persistence, replay, session membership,
// query readiness) into `health` / `health_compact`. Every field reads data
// that already exists after Gates A–L; nothing here recomputes index state.

use crate::domain::{
    CapabilityStatus, CapabilityUnavailableReason, CatalogEntry, CoverageStatus, FileDisposition,
    FreshnessStatus, ParseStatus, RepositoryManifest, SourceAccessMode, StateLocationKind,
    StatePlacement, UserLocalPlacementReason,
};
use crate::live_index::CodeSignalsSnapshot;
use crate::live_index::knowledge_authority::{KnowledgeAuthorityView, PolicyLedgerStatus};
use crate::live_index::knowledge_bridge::{DerivedCoverage, KnowledgeBridge};
use crate::live_index::store::PublishedSourceSet;

/// Reachable source-binding/runtime state for the M-001 health surface. Every
/// field is available on the per-session `SymForgeServer`; nothing here needs
/// daemon session-table plumbing.
pub struct SourceBindingHealthView<'a> {
    /// A source root is bound to this runtime (`repo_root` is `Some`).
    pub bound: bool,
    /// Resolved project-state placement, or `None` when unbound.
    pub placement: Option<&'a StatePlacement>,
    /// Runtime durability of the selected state owner (independent of readiness).
    pub persistence: CapabilityStatus,
    /// Resolved calling-session identity (real for a daemon session, a
    /// `local-process-<pid>` sentinel for the in-process path).
    pub session_id: &'a str,
    /// True when the report is served through a daemon session (membership
    /// authority applies); false for the local in-process path.
    pub daemon_session: bool,
    /// The live query generation is usable (`Ready`).
    pub query_ready: bool,
    /// Bounded `.gitignore` hygiene status word (read-only observe), or `None`
    /// when unbound. Computed at the call site so this module stays I/O-free.
    pub gitignore_hygiene: Option<&'a str>,
}

/// Short, stable prefix of a digest/id for bounded health display.
pub(crate) fn short_digest(value: &str) -> &str {
    // Char-boundary safe: digests/source-ids are ASCII hex today, but the
    // helper takes an arbitrary &str, so never byte-slice mid-codepoint.
    match value.char_indices().nth(12) {
        Some((idx, _)) => &value[..idx],
        None => value,
    }
}

pub(crate) fn coverage_label(coverage: CoverageStatus) -> &'static str {
    match coverage {
        CoverageStatus::Complete => "complete",
        CoverageStatus::Degraded => "degraded",
    }
}

pub(crate) fn placement_label(placement: Option<&StatePlacement>) -> &'static str {
    match placement {
        Some(StatePlacement::ProjectLocal { .. }) => "project_local",
        Some(StatePlacement::UserLocal { .. }) => "user_local",
        Some(StatePlacement::MemoryOnly { .. }) => "memory_only",
        None => "unbound",
    }
}

/// Authorization derived from the resolved placement, closed to the contract's
/// `normal | explicit_protected` set for any bound source.
///
/// `explicit_protected` always skips the project-local probe, so its state
/// placement is either `UserLocal{ExplicitProtected}` or a `MemoryOnly` whose
/// failures contain **no** `ProjectLocal` failure. A normal binding always
/// attempts project-local first, so a normal both-tier failure records a
/// `ProjectLocal` failure in `MemoryOnly` (see
/// `discovery::resolve_state_placement_with`). That makes the closed set fully
/// derivable from placement — no authoritative `SourceAccessMode` plumbing
/// needed. `None` is returned only for an unbound runtime (no placement).
pub(crate) fn placement_authorization(
    placement: Option<&StatePlacement>,
) -> Option<SourceAccessMode> {
    match placement {
        Some(StatePlacement::ProjectLocal { .. }) => Some(SourceAccessMode::NormalProject),
        Some(StatePlacement::UserLocal {
            reason: UserLocalPlacementReason::ExplicitProtected,
            ..
        }) => Some(SourceAccessMode::ExplicitProtected),
        Some(StatePlacement::UserLocal {
            reason: UserLocalPlacementReason::ProjectLocalUnavailable { .. },
            ..
        }) => Some(SourceAccessMode::NormalProject),
        Some(StatePlacement::UserLocal {
            reason: UserLocalPlacementReason::HostSelected,
            ..
        }) => Some(SourceAccessMode::NormalProject),
        Some(StatePlacement::MemoryOnly { failures }) => {
            if failures
                .iter()
                .any(|failure| failure.location == StateLocationKind::ProjectLocal)
            {
                Some(SourceAccessMode::NormalProject)
            } else {
                Some(SourceAccessMode::ExplicitProtected)
            }
        }
        None => None,
    }
}

pub(crate) fn authorization_label(placement: Option<&StatePlacement>) -> &'static str {
    match placement_authorization(placement) {
        Some(SourceAccessMode::NormalProject) => "normal",
        Some(SourceAccessMode::ExplicitProtected) => "explicit_protected",
        // Unbound only: no source is authorized, so the closed bound-source set
        // does not apply.
        None => "not_applicable",
    }
}

pub(crate) fn capability_unavailable_code(reason: CapabilityUnavailableReason) -> &'static str {
    match reason {
        CapabilityUnavailableReason::ExplicitProtectedSource => "explicit_protected_source",
        CapabilityUnavailableReason::SourceReadOnly => "source_read_only",
        CapabilityUnavailableReason::PersistentStateUnavailable => "persistent_state_unavailable",
        CapabilityUnavailableReason::DurableMutationReplayUnavailable => {
            "durable_mutation_replay_unavailable"
        }
        CapabilityUnavailableReason::NonProjectLocalPlacement => "non_project_local_placement",
        CapabilityUnavailableReason::AtomicDurabilityUnavailable => "atomic_durability_unavailable",
    }
}

/// Maps persistence capability to the source-binding contract's
/// healthy/degraded/disabled trichotomy. "disabled" means state was never
/// durable for this binding; "degraded" means a durable placement lost
/// durability after binding.
pub(crate) fn persistence_label(status: CapabilityStatus) -> String {
    match status {
        CapabilityStatus::Available => "healthy".to_string(),
        CapabilityStatus::Unavailable { reason } => {
            let state = match reason {
                CapabilityUnavailableReason::AtomicDurabilityUnavailable
                | CapabilityUnavailableReason::DurableMutationReplayUnavailable => "degraded",
                _ => "disabled",
            };
            format!("{state}({})", capability_unavailable_code(reason))
        }
    }
}

pub(crate) fn replay_label(
    placement: Option<&StatePlacement>,
    persistence: CapabilityStatus,
) -> &'static str {
    match (placement, persistence) {
        (
            Some(StatePlacement::ProjectLocal { .. } | StatePlacement::UserLocal { .. }),
            CapabilityStatus::Available,
        ) => "available",
        _ => "unavailable",
    }
}

pub(crate) fn render_source_binding_line(binding: &SourceBindingHealthView<'_>) -> String {
    let membership = if binding.daemon_session {
        "member"
    } else {
        "local_process"
    };
    if !binding.bound {
        // Unbound: placement/authorization/persistence/replay are bound-source
        // concepts whose closed enumerations have no unbound value, so they are
        // omitted rather than rendered out-of-set.
        return format!(
            "  binding=unbound query_readiness={} session={} membership={membership}",
            if binding.query_ready {
                "ready"
            } else {
                "not_ready"
            },
            binding.session_id,
        );
    }
    let mut line = format!(
        "  binding=bound authorization={} placement={} persistence={} replay={} query_readiness={} live_postcondition=established",
        authorization_label(binding.placement),
        placement_label(binding.placement),
        persistence_label(binding.persistence),
        replay_label(binding.placement, binding.persistence),
        if binding.query_ready {
            "ready"
        } else {
            "not_ready"
        },
    );
    // `live_postcondition=established`: a bound, attached session has a live
    // binding/generation. The transient `live_postcondition_unavailable` result
    // is an idempotency-replay outcome, not observable standing health. It is a
    // separate token from `query_readiness` on purpose — a bound binding can be
    // established while its generation is not yet queryable.
    if let Some(status) = binding.gitignore_hygiene {
        line.push_str(&format!(" gitignore_hygiene={status}"));
    }
    // Memory-only fallback reason codes (contract: "safe fallback reason codes").
    if let Some(StatePlacement::MemoryOnly { failures }) = binding.placement
        && !failures.is_empty()
    {
        let reasons: Vec<String> = failures
            .iter()
            .map(|failure| format!("{:?}:{:?}", failure.location, failure.safe_reason))
            .collect();
        line.push_str(&format!(" fallback_reasons=[{}]", reasons.join(",")));
    }
    line.push_str(&format!(
        " session={} membership={membership}",
        binding.session_id
    ));
    line
}

/// Coverage/digest tokens for one manifest. Shared by the primary manifest line
/// and the per-source source-set line so a source's terminal state is rendered
/// identically wherever it appears (M-002 terminal-disposition equality).
pub(crate) fn manifest_coverage_digest(manifest: Option<&RepositoryManifest>) -> String {
    match manifest {
        Some(manifest) => format!(
            "coverage={} digest={}",
            coverage_label(manifest.coverage),
            short_digest(&manifest.digest),
        ),
        None => "coverage=absent digest=none".to_string(),
    }
}

pub(crate) fn freshness_label(freshness: &FreshnessStatus) -> String {
    match freshness {
        FreshnessStatus::Current => "current".to_string(),
        FreshnessStatus::Verifying => "verifying".to_string(),
        FreshnessStatus::Degraded { reason_codes, .. } => {
            let codes: Vec<String> = reason_codes.iter().map(|r| format!("{r:?}")).collect();
            format!("degraded[{}]", codes.join(","))
        }
    }
}

/// Manifest line. A budget-degraded or unbound observation is reported as an
/// explicit `absent` marker (never a silent partial manifest — data-model.md
/// §ManifestResourceUsage: a partial observation "is not a `RepositoryManifest`").
pub(crate) fn render_manifest_line(
    manifest: Option<&RepositoryManifest>,
    freshness: &FreshnessStatus,
    inflight_budget_bytes: u64,
) -> String {
    match manifest {
        Some(manifest) => format!(
            "Manifest: {} freshness={} entries={} metadata_bytes={} admitted_bytes={} inflight_budget_bytes={}",
            manifest_coverage_digest(Some(manifest)),
            freshness_label(freshness),
            manifest.usage.catalog_entries,
            manifest.usage.catalog_metadata_bytes,
            manifest.usage.admitted_content_bytes,
            inflight_budget_bytes,
        ),
        None => format!(
            "Manifest: absent (unbound or budget-degraded source; no partial manifest is published) freshness={} inflight_budget_bytes={}",
            freshness_label(freshness),
            inflight_budget_bytes,
        ),
    }
}

/// Per-terminal-disposition counts across the manifest catalog. Every catalog
/// entry maps to exactly one terminal disposition, so a file is counted once.
pub(crate) fn render_disposition_line(entries: &[CatalogEntry]) -> String {
    let mut indexed = 0u64;
    let mut metadata_only = 0u64;
    let mut hard_skip = 0u64;
    let mut unreadable = 0u64;
    let mut unstable = 0u64;
    let mut aborted = 0u64;
    let mut parsed = 0u64;
    let mut partial = 0u64;
    let mut failed = 0u64;
    // Ingest-target mix over indexed entries (M-001 "target").
    let mut target_code = 0u64;
    let mut target_knowledge = 0u64;
    let mut target_both = 0u64;
    for entry in entries {
        match &entry.disposition {
            FileDisposition::Indexed {
                parse_status,
                targets,
            } => {
                indexed += 1;
                match parse_status {
                    ParseStatus::Parsed => parsed += 1,
                    ParseStatus::PartialParse => partial += 1,
                    ParseStatus::Failed => failed += 1,
                }
                match (targets.includes_code(), targets.includes_knowledge()) {
                    (true, true) => target_both += 1,
                    (true, false) => target_code += 1,
                    (false, _) => target_knowledge += 1,
                }
            }
            FileDisposition::MetadataOnly { .. } => metadata_only += 1,
            FileDisposition::HardSkip { .. } => hard_skip += 1,
            FileDisposition::Unreadable { .. } => unreadable += 1,
            FileDisposition::UnstableDuringRead => unstable += 1,
            FileDisposition::AbortedCircuitBreaker => aborted += 1,
        }
    }
    format!(
        "Dispositions: indexed={indexed} metadata_only={metadata_only} hard_skip={hard_skip} unreadable={unreadable} unstable={unstable} aborted={aborted} (parse parsed={parsed} partial={partial} failed={failed}) (targets code={target_code} knowledge={target_knowledge} both={target_both})"
    )
}

/// One source-set entry line. Uses [`manifest_coverage_digest`] so the current
/// source renders identically here and in the primary manifest line.
pub(crate) fn render_source_entry_line(
    source_id_short: &str,
    is_current: bool,
    publication_generation: u64,
    content_generation: u64,
    project_generation: u64,
    manifest: Option<&RepositoryManifest>,
) -> String {
    format!(
        "  {}{} gen=pub{}/content{}/proj{} {} entries={}",
        source_id_short,
        if is_current { "*" } else { "" },
        publication_generation,
        content_generation,
        project_generation,
        manifest_coverage_digest(manifest),
        manifest.map(|m| m.usage.catalog_entries).unwrap_or(0),
    )
}

pub(crate) const MAX_HEALTH_SOURCES: usize = 8;

pub(crate) fn render_source_set_section(source_set: &PublishedSourceSet) -> String {
    let mut out = format!(
        "Source set: registry_generation={} current={} sources={}",
        source_set.registry_generation,
        short_digest(source_set.current_source_id.as_str()),
        source_set.sources.len(),
    );
    for (source_id, generation) in source_set.sources.iter().take(MAX_HEALTH_SOURCES) {
        out.push('\n');
        out.push_str(&render_source_entry_line(
            short_digest(source_id.as_str()),
            *source_id == source_set.current_source_id,
            generation.publication_generation,
            generation.content_generation,
            generation.project_generation,
            generation.manifest.as_deref(),
        ));
    }
    if source_set.sources.len() > MAX_HEALTH_SOURCES {
        out.push_str(&format!(
            "\n  … {} more source(s) omitted",
            source_set.sources.len() - MAX_HEALTH_SOURCES
        ));
    }
    out
}

pub(crate) fn derived_coverage_label(coverage: &DerivedCoverage) -> String {
    match coverage {
        DerivedCoverage::Complete => "complete".to_string(),
        DerivedCoverage::Truncated { breaches } => {
            let omitted: u64 = breaches.iter().map(|breach| breach.omitted).sum();
            format!("truncated({omitted} omitted)")
        }
        DerivedCoverage::Loading => "loading".to_string(),
    }
}

pub(crate) fn render_bridge_line(bridge: &KnowledgeBridge, content_generation: u64) -> String {
    format!(
        "Bridge: coverage={} cards={} forward_links={} knowledge_links={} version=content{}",
        derived_coverage_label(&bridge.coverage),
        bridge.cards.len(),
        bridge.forward.len(),
        bridge.knowledge_links.len(),
        content_generation,
    )
}

pub(crate) fn render_temporal_line(code_signals: &CodeSignalsSnapshot) -> String {
    use crate::live_index::git_temporal::GitTemporalState;
    let state = match &code_signals.state {
        GitTemporalState::Pending => "pending".to_string(),
        GitTemporalState::Computing => "computing".to_string(),
        GitTemporalState::Unavailable(reason) => format!("unavailable({reason})"),
        GitTemporalState::Ready => "ready".to_string(),
    };
    let coverage = if code_signals.coverage.complete_to_root {
        "complete_to_root".to_string()
    } else if code_signals.coverage.limitations.is_empty() {
        "incomplete".to_string()
    } else {
        let limits: Vec<String> = code_signals
            .coverage
            .limitations
            .iter()
            .map(|limit| format!("{limit:?}"))
            .collect();
        format!("limited[{}]", limits.join(","))
    };
    format!("Temporal: state={state} coverage={coverage}")
}

pub(crate) fn policy_status_label(status: &PolicyLedgerStatus) -> String {
    match status {
        PolicyLedgerStatus::Absent => "absent".to_string(),
        PolicyLedgerStatus::Valid => "valid".to_string(),
        PolicyLedgerStatus::Malformed => "malformed".to_string(),
        PolicyLedgerStatus::UnsupportedVersion { found } => format!(
            "unsupported_version({})",
            found
                .map(|version| version.to_string())
                .unwrap_or_else(|| "none".to_string())
        ),
        PolicyLedgerStatus::InvalidEntries => "invalid_entries".to_string(),
    }
}

pub(crate) fn render_authority_hygiene_line(authority: &KnowledgeAuthorityView) -> String {
    // Policy status is not observed while the bridge is still publishing.
    // Printing Absent here would claim a ledger read that did not happen.
    if matches!(authority.coverage, DerivedCoverage::Loading) {
        return "Authority hygiene: coverage=loading curation_eligible=false".to_string();
    }
    format!(
        "Authority hygiene: rule_version={} policy_version={} secret_policy_version={} policy_status={} curation_eligible={} records={} filtered_suppressions={} coverage={}",
        authority.versions.authority_rule_version,
        authority.versions.policy_version,
        authority.versions.secret_policy_version,
        policy_status_label(&authority.policy_status),
        authority.curation_eligible,
        authority.records.len(),
        authority.skipped_suppression_ids.len(),
        derived_coverage_label(&authority.coverage),
    )
}

/// Full Feature 020 repository-knowledge health section (M-001). Extends the
/// `health` report; every field is read from the already-published source set
/// plus the reachable source-binding state.
pub fn format_repository_knowledge_health(
    source_set: &PublishedSourceSet,
    binding: &SourceBindingHealthView<'_>,
    inflight_budget_bytes: u64,
) -> String {
    let generation = source_set.current_generation();
    let mut section = String::from("── Repository knowledge (Feature 020) ──\n");
    section.push_str(&render_source_binding_line(binding));
    section.push('\n');
    section.push_str(&render_manifest_line(
        generation.manifest.as_deref(),
        generation.freshness.as_ref(),
        inflight_budget_bytes,
    ));
    if let Some(manifest) = generation.manifest.as_deref() {
        section.push('\n');
        section.push_str(&render_disposition_line(&manifest.entries));
    }
    section.push('\n');
    section.push_str(&render_source_set_section(source_set));
    section.push('\n');
    section.push_str(&render_bridge_line(
        generation.bridge.as_ref(),
        generation.content_generation,
    ));
    section.push('\n');
    section.push_str(&render_temporal_line(generation.code_signals.as_ref()));
    section.push('\n');
    section.push_str(&render_authority_hygiene_line(
        generation.authority.as_ref(),
    ));
    section
}

/// Compact Feature 020 repository-knowledge health (M-001). Two lines: a binding
/// summary and a one-line knowledge digest. Kept genuinely compact.
pub fn format_repository_knowledge_health_compact(
    source_set: &PublishedSourceSet,
    binding: &SourceBindingHealthView<'_>,
) -> String {
    let generation = source_set.current_generation();
    let manifest_summary = match generation.manifest.as_deref() {
        Some(manifest) => format!("manifest={}", coverage_label(manifest.coverage)),
        None => "manifest=absent".to_string(),
    };
    let readiness = if binding.query_ready {
        "ready"
    } else {
        "not_ready"
    };
    // Unbound: omit bound-only closed-set fields (placement/authorization/…).
    let binding_line = if binding.bound {
        format!(
            "Source binding: binding=bound authorization={} placement={} persistence={} replay={} readiness={} gitignore={}",
            authorization_label(binding.placement),
            placement_label(binding.placement),
            persistence_label(binding.persistence),
            replay_label(binding.placement, binding.persistence),
            readiness,
            binding.gitignore_hygiene.unwrap_or("n/a"),
        )
    } else {
        format!("Source binding: binding=unbound readiness={readiness}")
    };
    format!(
        "{binding_line}\nKnowledge: {} sources={} bridge={} authority(rule/policy/secret)={}/{}/{}",
        manifest_summary,
        source_set.sources.len(),
        derived_coverage_label(&generation.bridge.coverage),
        generation.authority.versions.authority_rule_version,
        generation.authority.versions.policy_version,
        generation.authority.versions.secret_policy_version,
    )
}

pub fn health_report_from_stats(
    status: &str,
    stats: &HealthStats,
    rejected_stale_mutations: u64,
) -> String {
    health_report_from_stats_windowed(
        status,
        stats,
        rejected_stale_mutations,
        QuarantineWindow::from_args(None, None),
    )
}

/// Like [`health_report_from_stats`] but renders the quarantine registry through
/// the given display window (SF-STRESS-010 retrieval surface).
pub fn health_report_from_stats_windowed(
    status: &str,
    stats: &HealthStats,
    rejected_stale_mutations: u64,
    quarantine_window: QuarantineWindow,
) -> String {
    use crate::watcher_state::WatcherState;

    let relative_age = |time: Option<std::time::SystemTime>| -> String {
        match time {
            None => "never".to_string(),
            Some(t) => {
                let secs = t.elapsed().map(|d| d.as_secs()).unwrap_or(0);
                format!("{secs}s ago")
            }
        }
    };

    let watcher_line = match &stats.watcher_state {
        WatcherState::Off if is_local_fallback_stats(stats) => {
            "Watcher: local-fallback (no watcher attached; daemon proxy unavailable)".to_string()
        }
        WatcherState::Active
            if stats.events_processed == 0
                && stats.last_event_at.is_none()
                && stats.overflow_count == 0
                && stats.stale_files_found == 0 =>
        {
            format!(
                "Watcher: active (idle; event-driven, waiting for filesystem changes, debounce: {}ms)",
                stats.debounce_window_ms
            )
        }
        WatcherState::Active if stats.events_processed == 0 && stats.last_event_at.is_none() => {
            format!(
                "Watcher: active (idle; debounce: {}ms, overflows: {}, reconcile repairs: {}, last reconcile: {})",
                stats.debounce_window_ms,
                stats.overflow_count,
                stats.stale_files_found,
                relative_age(stats.last_reconcile_at)
            )
        }
        WatcherState::Active => format!(
            "Watcher: active (event-driven; {} events, last change: {}, debounce: {}ms, overflows: {}, reconcile repairs: {}, last overflow: {}, last reconcile: {})",
            stats.events_processed,
            relative_age(stats.last_event_at),
            stats.debounce_window_ms,
            stats.overflow_count,
            stats.stale_files_found,
            relative_age(stats.last_overflow_at),
            relative_age(stats.last_reconcile_at)
        ),
        WatcherState::Starting => {
            "Watcher: starting (registering filesystem watch; index will stay fresh once the watch is registered)".to_string()
        }
        WatcherState::Degraded => format!(
            "Watcher: degraded (event stream failed after {} processed events, overflows: {}, reconcile repairs: {}, last overflow: {}, last reconcile: {})",
            stats.events_processed,
            stats.overflow_count,
            stats.stale_files_found,
            relative_age(stats.last_overflow_at),
            relative_age(stats.last_reconcile_at)
        ),
        WatcherState::Off => "Watcher: off".to_string(),
    };

    let (tier1, tier2, tier3) = stats.tier_counts;
    let total_discovered = tier1 + tier2 + tier3;
    let mut admission_section = format!(
        "\nAdmission: {} files discovered (after gitignore/global excludes)\n  Tier 1 (indexed): {}\n  Tier 2 (metadata only): {}\n  Tier 3 (hard-skipped): {}",
        total_discovered, tier1, tier2, tier3
    );
    // SF-009: surface how many Tier-1 files are not under version control.
    // Shown only when > 0 to keep clean, fully-tracked repos quiet; the count
    // fails open to 0 (and is therefore hidden) outside a git working tree.
    if stats.untracked_indexed > 0 {
        admission_section.push_str(&format!(
            "\n  indexed untracked files: {}",
            stats.untracked_indexed
        ));
    }

    let mut output = format!(
        "Status: {}\nFiles:  {} indexed ({} parsed, {} partial, {} failed)\nSymbols: {}\nLoaded in: {}ms\n{}\nStale-mutation rejections: {}{}",
        status,
        stats.file_count,
        stats.parsed_count,
        stats.partial_parse_count,
        stats.failed_count,
        stats.symbol_count,
        stats.load_duration.as_millis(),
        watcher_line,
        rejected_stale_mutations,
        admission_section
    );

    if let Some(reason) = stats.local_empty_reason.as_deref()
        && stats.file_count == 0
    {
        let banner = format!(
            "\n\n⚠ Empty index — {reason}\n  Recovery: call index_folder(path=\"<your-project-root>\") or restart with --root <path>"
        );
        output.push_str(&banner);
    }

    if stats.partial_parse_count > 0 {
        // `unexpected` is the ACTIONABLE category, so it is always stated —
        // "0 unexpected" is precisely the reassurance an operator is scanning
        // for, and suppressing it would make its absence ambiguous.
        //
        // The six expected_* buckets are a different case: they are almost
        // always zero, and a zero there carries no information at all. On this
        // repo the line spent five of its seven clauses saying nothing. Render
        // only the buckets that actually fired, and drop the heuristic-label
        // footnote entirely when none did — it explains labels that are not on
        // screen.
        let expected = [
            ("expected vendor", stats.expected_vendor_partial_parse_count),
            (
                "expected generated",
                stats.expected_generated_partial_parse_count,
            ),
            (
                "expected test-fixture",
                stats.expected_test_fixture_partial_parse_count,
            ),
            (
                "expected template-DSL",
                stats.expected_template_dsl_partial_parse_count,
            ),
            (
                "expected framework",
                stats.expected_framework_partial_parse_count,
            ),
            (
                "expected language",
                stats.expected_language_partial_parse_count,
            ),
        ];
        let mut parts = vec![format!(
            "{} unexpected",
            stats.unexpected_partial_parse_count
        )];
        parts.extend(
            expected
                .iter()
                .filter(|(_, count)| *count > 0)
                .map(|(label, count)| format!("{count} {label}")),
        );
        let footnote = if expected.iter().any(|(_, count)| *count > 0) {
            " (expected_* vendor/generated/test-fixture/template-DSL are heuristic path-based labels)"
        } else {
            ""
        };
        output.push_str(&format!(
            "\nPartial parse summary: {}{footnote}",
            parts.join(", ")
        ));
    }

    if stats.failed_count > 0 {
        output.push_str(
            "\nParse resilience: failed files are excluded from symbol-level answers until they re-index cleanly. Inspect the failed-file list below, use raw reads or validate_file_syntax for those paths, then re-run index_folder after fixing the source file.",
        );
    } else if stats.unexpected_partial_parse_count > 0 {
        output.push_str(
            "\nParse resilience: partial files kept best-effort symbols; unexpected repo-owned partials remain visible below and may indicate missing symbols.",
        );
    } else if stats.expected_vendor_partial_parse_count > 0 {
        output.push_str(
            "\nParse resilience: expected vendor partial files kept best-effort symbols; they are labeled as vendor parser noise below.",
        );
    } else if stats.expected_framework_partial_parse_count > 0 {
        output.push_str(
            "\nParse resilience: expected framework partial files kept best-effort symbols; they are labeled as framework template parser noise below.",
        );
    } else if stats.expected_language_partial_parse_count > 0 {
        output.push_str(
            "\nParse resilience: expected language partial files kept best-effort symbols; they are labeled as host-language grammar parser noise below.",
        );
    }

    // The Parse/span quarantine registry below lists ranked quarantined paths
    // across ALL categories with category labels, windowed by `quarantine_window`
    // (default offset 0, limit PARSE_QUARANTINE_ENTRY_LIMIT; raised by the
    // `health` quarantine_limit/quarantine_offset params — SF-STRESS-010). The
    // per-category sections that follow render only paths NOT already shown in
    // the current window (the genuinely-omitted overflow), deduping against the
    // registry and skipping entirely when the registry already shows everything.
    let quarantine = ParseQuarantineSummary::from_stats(stats)
        .with_window(quarantine_window.offset, quarantine_window.limit);
    let shown_in_registry = quarantine.shown_paths();
    if let Some(section) = quarantine.full_section() {
        output.push('\n');
        output.push_str(&section);
    }

    // Helper: render a per-category overflow section listing only the paths that
    // the registry omitted. `decorate` formats each path line body (without the
    // leading "  N. " counter).
    let render_overflow = |output: &mut String,
                           header: &str,
                           overflow_noun: &str,
                           paths: &[String],
                           decorate: &dyn Fn(&str) -> String| {
        let remaining: Vec<&String> = paths
            .iter()
            .filter(|p| !shown_in_registry.contains(p.as_str()))
            .collect();
        if remaining.is_empty() {
            return;
        }
        output.push_str(&format!("\n{} ({}):\n", header, remaining.len()));
        for (i, path) in remaining.iter().take(10).enumerate() {
            output.push_str(&format!("  {}. {}\n", i + 1, decorate(path)));
        }
        let omitted = remaining.len().saturating_sub(10);
        if omitted > 0 {
            output.push_str(&format!("  ... and {} more {}\n", omitted, overflow_noun));
        }
    };

    if stats.unexpected_partial_parse_count > 0 {
        render_overflow(
            &mut output,
            "Unexpected repo-owned partial parse files (not shown above)",
            "unexpected partial files",
            &stats.unexpected_partial_parse_files,
            &|path| path.to_string(),
        );
    }

    if stats.expected_vendor_partial_parse_count > 0 {
        render_overflow(
            &mut output,
            "Expected vendor partial parse noise (heuristic, not shown above)",
            "expected vendor partial files",
            &stats.expected_vendor_partial_parse_files,
            &|path| format!("{} [{}]", path, EXPECTED_VENDOR_PARTIAL_PARSE_REASON),
        );
    }

    if stats.expected_generated_partial_parse_count > 0 {
        render_overflow(
            &mut output,
            "Expected generated partial parse noise (heuristic, not shown above)",
            "expected generated partial files",
            &stats.expected_generated_partial_parse_files,
            &|path| format!("{} [{}]", path, EXPECTED_GENERATED_PARTIAL_PARSE_REASON),
        );
    }

    if stats.expected_test_fixture_partial_parse_count > 0 {
        render_overflow(
            &mut output,
            "Expected test-fixture partial parse noise (heuristic, not shown above)",
            "expected test-fixture partial files",
            &stats.expected_test_fixture_partial_parse_files,
            &|path| format!("{} [{}]", path, EXPECTED_TEST_FIXTURE_PARTIAL_PARSE_REASON),
        );
    }

    if stats.expected_template_dsl_partial_parse_count > 0 {
        render_overflow(
            &mut output,
            "Expected template-DSL partial parse noise (heuristic, not shown above)",
            "expected template-DSL partial files",
            &stats.expected_template_dsl_partial_parse_files,
            &|path| format!("{} [{}]", path, EXPECTED_TEMPLATE_DSL_PARTIAL_PARSE_REASON),
        );
    }

    if stats.expected_framework_partial_parse_count > 0 {
        render_overflow(
            &mut output,
            "Expected framework partial parse noise (not shown above)",
            "expected framework partial files",
            &stats.expected_framework_partial_parse_files,
            &|path| format!("{} [{}]", path, EXPECTED_FRAMEWORK_PARTIAL_PARSE_REASON),
        );
    }

    if stats.expected_language_partial_parse_count > 0 {
        render_overflow(
            &mut output,
            "Expected language partial parse noise (not shown above)",
            "expected language partial files",
            &stats.expected_language_partial_parse_files,
            &|path| format!("{} [{}]", path, EXPECTED_LANGUAGE_PARTIAL_PARSE_REASON),
        );
    }

    if !stats.failed_files.is_empty() {
        let remaining: Vec<&(String, String)> = stats
            .failed_files
            .iter()
            .filter(|(path, _)| !shown_in_registry.contains(path.as_str()))
            .collect();
        if !remaining.is_empty() {
            output.push_str(&format!(
                "\nFailed files (not shown above) ({}):\n",
                remaining.len()
            ));
            for (i, (path, error)) in remaining.iter().take(10).enumerate() {
                output.push_str(&format!("  {}. {} — {}\n", i + 1, path, error));
            }
            if remaining.len() > 10 {
                output.push_str(&format!(
                    "  ... and {} more failed files\n",
                    remaining.len() - 10
                ));
            }
        }
    }

    output
}

pub(crate) fn is_local_fallback_stats(stats: &HealthStats) -> bool {
    matches!(stats.watcher_state, crate::watcher_state::WatcherState::Off)
        && stats.events_processed == 0
        && stats.last_event_at.is_none()
        && stats.debounce_window_ms == 0
        && stats.overflow_count == 0
        && stats.last_overflow_at.is_none()
        && stats.stale_files_found == 0
        && stats.last_reconcile_at.is_none()
}

/// Format a one-line git temporal summary for the health report.
pub fn git_temporal_health_line(
    temporal: &crate::live_index::git_temporal::GitTemporalIndex,
) -> String {
    use crate::live_index::git_temporal::GitTemporalState;

    match &temporal.state {
        GitTemporalState::Pending => "Git temporal: pending".to_string(),
        GitTemporalState::Computing => "Git temporal: computing...".to_string(),
        GitTemporalState::Unavailable(reason) => {
            format!("Git temporal: unavailable ({reason})")
        }
        GitTemporalState::Ready => {
            let stats = &temporal.stats;
            let mut lines = vec![format!(
                "Git temporal: ready ({} commits over {}d, computed in {}ms)",
                stats.total_commits_analyzed,
                stats.analysis_window_days,
                stats.compute_duration.as_millis(),
            )];

            if !stats.hotspots.is_empty() {
                let top: Vec<String> = stats
                    .hotspots
                    .iter()
                    .take(5)
                    .map(|(path, score)| format!("{path} ({score:.2})"))
                    .collect();
                lines.push(format!("  Hotspots: {}", top.join(", ")));
            }

            if !stats.most_coupled.is_empty() {
                let (a, b, score) = &stats.most_coupled[0];
                lines.push(format!(
                    "  Strongest coupling: {a} \u{2194} {b} ({score:.2})"
                ));
            }

            lines.join("\n")
        }
    }
}

/// Render the "Top frecent files" section for the health report.
///
/// `entries` is a pre-sorted list of `(path, decayed_score)` from
/// `FrecencyStore::top_frecent`. When empty, returns a short "no data yet"
/// line so operators can tell the section is live but unpopulated.
pub fn format_frecency_top(entries: &[(std::path::PathBuf, f64)]) -> String {
    let mut lines = vec!["── Top frecent files ──".to_string()];
    if entries.is_empty() {
        lines.push("  (no frecency rows recorded yet)".to_string());
    } else {
        for (path, score) in entries {
            lines.push(format!("  {:.2}  {}", score, path.display()));
        }
    }
    lines.join("\n")
}

/// Render the "Last 10 frecency bumps" debug section for the health report.
///
/// Gated at the call-site by the ranking diagnostics default-on policy.
/// `entries` comes from `FrecencyStore::last_10_bumps` (already ordered
/// newest-first). Empty input produces a short "no data yet" line.
pub fn format_frecency_last_bumps(entries: &[crate::live_index::frecency::BumpEntry]) -> String {
    let mut lines = vec!["── Last 10 frecency bumps ──".to_string()];
    if entries.is_empty() {
        lines.push("  (no frecency rows recorded yet)".to_string());
    } else {
        for e in entries {
            lines.push(format!(
                "  ts={} hits={}  {}",
                e.last_access_ts,
                e.hit_count,
                e.path.display(),
            ));
        }
    }
    lines.join("\n")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CapabilityStatusReport {
    pub(crate) frecency: String,
    pub(crate) co_change: String,
    pub(crate) worktree_routing: String,
    pub(crate) ranking_diagnostics: String,
}

impl CapabilityStatusReport {
    pub(crate) fn full_text(&self) -> String {
        format!(
            "Capabilities:\n  frecency: {}\n  co-change: {}\n  worktree routing: {}\n  ranking diagnostics: {}",
            self.frecency, self.co_change, self.worktree_routing, self.ranking_diagnostics
        )
    }

    pub(crate) fn compact_text(&self) -> String {
        format!(
            "Capabilities: frecency={}; co-change={}; worktree={}; ranking={}",
            self.frecency, self.co_change, self.worktree_routing, self.ranking_diagnostics
        )
    }
}

pub(crate) fn frecency_health_status(
    repo_root: Option<&std::path::Path>,
    project_state: Option<&crate::domain::ProjectStateDir>,
) -> String {
    if repo_root.is_none() {
        return "unavailable/no-repository-root".to_string();
    }
    let has_persistent_history = project_state
        .map(crate::live_index::frecency::frecency_db_path)
        .is_some_and(|path| path.is_file());
    match crate::live_index::frecency::collection_policy_from_env() {
        crate::capability::FrecencyCollectionPolicy::Disabled => "disabled by policy".to_string(),
        crate::capability::FrecencyCollectionPolicy::Session => {
            if has_persistent_history {
                "ready/session+persistent".to_string()
            } else {
                "ready/session/no-history fallback-used-on-empty".to_string()
            }
        }
        crate::capability::FrecencyCollectionPolicy::Persistent => {
            if project_state.is_none() {
                return "unavailable/no-project-state-owner".to_string();
            }
            if has_persistent_history {
                "ready/persistent".to_string()
            } else {
                "ready/persistent/no-history fallback-used-on-empty".to_string()
            }
        }
    }
}

pub(crate) fn cochange_store_health_status(
    store: &crate::live_index::coupling::CouplingStore,
    repo_root: &std::path::Path,
) -> String {
    match store.cold_built_at() {
        Ok(Some(_)) => {}
        Ok(None) => return "preparing/cold-build-pending fallback-used-on-request".to_string(),
        Err(error) => return format!("unavailable/store-build-state-error ({error})"),
    }

    let stored_head = match store.last_head() {
        Ok(Some(head)) => head,
        Ok(None) => return "preparing/no-head-recorded fallback-used-on-request".to_string(),
        Err(error) => return format!("unavailable/store-head-state-error ({error})"),
    };
    let current_head = match crate::git::head_sha(repo_root) {
        Ok(head) => head,
        Err(error) => return format!("unavailable/head-read-failed ({error})"),
    };
    if stored_head != current_head {
        "stale/head-mismatch fallback-used-on-request".to_string()
    } else {
        "ready/current".to_string()
    }
}

pub(crate) fn cochange_health_status(
    index: &crate::live_index::LiveIndex,
    repo_root: Option<&std::path::Path>,
    project_state: Option<&crate::domain::ProjectStateDir>,
) -> String {
    let policy = crate::live_index::coupling::coupling_prepare_policy_from_env();
    if matches!(policy, crate::capability::CouplingPreparePolicy::Disabled) {
        return "disabled by policy".to_string();
    }

    let Some(repo_root) = repo_root else {
        return "unavailable/no-repository-root".to_string();
    };
    if git2::Repository::discover(repo_root).is_err() {
        return "unavailable/not-a-git-repo".to_string();
    }

    if let Some(store) = index.coupling_store() {
        return cochange_store_health_status(store, repo_root);
    }

    let Some(project_state) = project_state else {
        return "unavailable/no-project-state-owner".to_string();
    };
    match crate::live_index::coupling::open_existing_coupling_store(project_state) {
        Ok(Some(store)) => cochange_store_health_status(store.as_ref(), repo_root),
        Ok(None) => match policy {
            crate::capability::CouplingPreparePolicy::LazyOnRequest => {
                "preparing/lazy-on-request fallback-used-on-request".to_string()
            }
            crate::capability::CouplingPreparePolicy::WarmOnStart => {
                "preparing/warm-on-start fallback-used-on-request".to_string()
            }
            crate::capability::CouplingPreparePolicy::Disabled => "disabled by policy".to_string(),
        },
        Err(error) => format!("unavailable/store-open-failed ({error})"),
    }
}

pub(crate) fn ranking_diagnostics_health_status() -> String {
    match crate::index_lifecycle::guidance::file_search::ranking_diagnostics_policy_from_env() {
        crate::capability::RankingDiagnosticsPolicy::CallTimeExplain => {
            "call-time explain available/default-off".to_string()
        }
        crate::capability::RankingDiagnosticsPolicy::DefaultOn => {
            "call-time explain available/default-on".to_string()
        }
        crate::capability::RankingDiagnosticsPolicy::Disabled => "disabled by policy".to_string(),
    }
}

/// The `Capabilities:` health section. `worktree_routing` is supplied by the
/// caller because worktree routing is a server-transport policy; embedded
/// hosts pass their explicit not-applicable reason.
pub(crate) fn capability_status_report(
    index: &crate::live_index::LiveIndex,
    repo_root: Option<&std::path::Path>,
    project_state: Option<&crate::domain::ProjectStateDir>,
    worktree_routing: String,
) -> CapabilityStatusReport {
    CapabilityStatusReport {
        frecency: frecency_health_status(repo_root, project_state),
        co_change: cochange_health_status(index, repo_root, project_state),
        worktree_routing,
        ranking_diagnostics: ranking_diagnostics_health_status(),
    }
}

/// Read-only `.gitignore` hygiene status for the health `gitignore_hygiene=`
/// field (source-binding-and-state.md Health contract). `None` when unbound.
/// Explicit-protected roots are not applicable (hygiene never touches a
/// protected root); every other bound root is observed without mutation via
/// `ObserveOnly` — the same code path bind/init use, never a second checker.
pub(crate) fn gitignore_hygiene_status(
    repo_root: Option<&std::path::Path>,
    placement: Option<&crate::domain::StatePlacement>,
) -> Option<&'static str> {
    let root = repo_root?;
    if matches!(
        placement_authorization(placement),
        Some(crate::domain::SourceAccessMode::ExplicitProtected)
    ) {
        return Some("not_applicable_explicit_protected");
    }
    Some(
        crate::gitignore_hygiene::reconcile_project_gitignore(
            root,
            crate::gitignore_hygiene::GitignoreHygieneAuthority::ObserveOnly,
        )
        .status_label(),
    )
}

/// One health/status line carrying ONLY the secret-dismissal record count:
/// never paths, rule ids, digests, or notes (034 no-echo contract).
pub(crate) fn secret_dismissals_line(root: Option<&std::path::Path>) -> String {
    match root.map(crate::knowledge::secret_dismissals::dismissal_count) {
        None => "secret_dismissals: unbound".to_string(),
        Some(Some(count)) => format!("secret_dismissals: {count}"),
        Some(None) => "secret_dismissals: store unreadable (all findings withheld)".to_string(),
    }
}

// ── Shared `health` / `health_compact` assembly ─────────────────────────────

/// Everything the shared health sections read, captured once by the caller so
/// one report never mixes publications (T046).
pub(crate) struct HealthReportInputs<'a> {
    pub source_set: &'a PublishedSourceSet,
    pub watcher: &'a crate::watcher_state::WatcherInfo,
    pub rejected_stale_mutations: u64,
    pub runtime_status: &'a RuntimeStatus,
    pub repo_root: Option<&'a std::path::Path>,
    pub placement: Option<&'a StatePlacement>,
    pub persistence: CapabilityStatus,
    /// The report is served through a daemon session (membership applies).
    pub session_is_daemon: bool,
    /// Worktree-routing policy text; a server-transport policy the caller owns.
    pub worktree_routing: String,
    /// The knowledge-curation recovery line from the caller's coordinator.
    pub curation_health: String,
}

/// Sections only a server process can observe. The MCP handler renders its
/// observations; an embedded host renders explicit not-applicable text, never
/// an omission.
pub(crate) struct HealthProcessSections {
    /// Appended directly after the runtime line (trust diagnostics, sidecar,
    /// token savings, tool calls, hook adoption). Each part carries its own
    /// leading newline.
    pub after_runtime: String,
    /// Rolling-hour count of edit calls without `working_directory`, or the
    /// reason the counter does not exist on this surface.
    pub worktree_misuse: Result<u64, &'static str>,
    /// Appended at the end (version drift, PATH shadow), own leading newlines.
    pub trailing: String,
}

fn health_capabilities(inputs: &HealthReportInputs<'_>) -> CapabilityStatusReport {
    let generation = inputs.source_set.current_generation();
    capability_status_report(
        &generation.live,
        inputs.repo_root,
        inputs.placement.and_then(StatePlacement::directory),
        inputs.worktree_routing.clone(),
    )
}

fn health_binding_view<'a>(
    inputs: &'a HealthReportInputs<'a>,
    query_ready: bool,
) -> SourceBindingHealthView<'a> {
    SourceBindingHealthView {
        bound: inputs.repo_root.is_some(),
        placement: inputs.placement,
        persistence: inputs.persistence,
        session_id: &inputs.runtime_status.session_id,
        daemon_session: inputs.session_is_daemon,
        query_ready,
        gitignore_hygiene: gitignore_hygiene_status(inputs.repo_root, inputs.placement),
    }
}

/// The full `health` report. Section order is the MCP contract.
pub(crate) fn render_health_report(
    inputs: &HealthReportInputs<'_>,
    quarantine_window: QuarantineWindow,
    process: &HealthProcessSections,
) -> String {
    let generation = inputs.source_set.current_generation();
    let published = &generation.health;
    let mut result = health_report_from_published_state_windowed(
        published,
        inputs.watcher,
        inputs.rejected_stale_mutations,
        quarantine_window,
    );
    result.push('\n');
    result.push_str(&format_runtime_status(inputs.runtime_status));
    result.push_str(&process.after_runtime);
    result.push('\n');
    result.push_str(&git_temporal_health_line(&generation.code_signals.temporal));
    result.push('\n');
    result.push_str(&health_capabilities(inputs).full_text());
    result.push('\n');
    result.push_str(&inputs.curation_health);
    // Feature 020 repository-knowledge health (M-001), from the entry capture.
    result.push('\n');
    result.push_str(&format_repository_knowledge_health(
        inputs.source_set,
        &health_binding_view(inputs, published.status_label() == "Ready"),
        crate::live_index::store::configured_inflight_byte_budget(),
    ));
    result.push('\n');
    result.push_str(&format!(
        "── Worktree-awareness misuse ──\nedit tool calls without working_directory (last hour): {}",
        match process.worktree_misuse {
            Ok(count) => count.to_string(),
            Err(reason) => format!("not_applicable ({reason})"),
        }
    ));
    result.push('\n');
    result.push_str(&secret_dismissals_line(inputs.repo_root));

    // Frecency diagnostics when SYMFORGE_FRECENCY=1 (mirrors `frecency::bump`).
    if std::env::var(crate::live_index::frecency::FRECENCY_FLAG_ENV).as_deref() == Ok("1")
        && let Some(project_state) = inputs.placement.and_then(StatePlacement::directory)
        && let Ok(store) = crate::live_index::frecency::FrecencyStore::open(
            &crate::live_index::frecency::frecency_db_path(project_state),
        )
    {
        let now_ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        if let Ok(top) = store.top_frecent(10, now_ts) {
            result.push('\n');
            result.push_str(&format_frecency_top(&top));
        }
        // "Last 10 frecency bumps" is a debug-only ranker-tuning surface,
        // additionally gated on the ranking-diagnostics default-on policy.
        if crate::index_lifecycle::guidance::file_search::ranking_diagnostics_policy_from_env()
            == crate::capability::RankingDiagnosticsPolicy::DefaultOn
            && let Ok(last) = store.last_10_bumps()
        {
            result.push('\n');
            result.push_str(&format_frecency_last_bumps(&last));
        }
    }
    result.push_str(&process.trailing);
    result
}

/// The `health_compact` projection of the same inputs.
pub(crate) fn render_health_compact(
    inputs: &HealthReportInputs<'_>,
    process: &HealthProcessSections,
) -> String {
    let generation = inputs.source_set.current_generation();
    let published = &generation.health;
    let mut result = health_report_compact_from_published_state(
        published,
        inputs.watcher,
        inputs.rejected_stale_mutations,
    );
    result.push('\n');
    result.push_str(&format_runtime_status_compact(inputs.runtime_status));
    result.push_str(&process.after_runtime);
    let git_temporal = git_temporal_health_line(&generation.code_signals.temporal);
    let git_temporal_summary = git_temporal
        .lines()
        .next()
        .unwrap_or("Git temporal: unknown");
    result.push_str(&format!(
        "\n{} | Worktree misuse/hour: {}",
        git_temporal_summary,
        match process.worktree_misuse {
            Ok(count) => count.to_string(),
            Err(_) => "not_applicable".to_string(),
        }
    ));
    result.push('\n');
    result.push_str(&health_capabilities(inputs).compact_text());
    result.push('\n');
    result.push_str(&inputs.curation_health);
    result.push('\n');
    result.push_str(&format_repository_knowledge_health_compact(
        inputs.source_set,
        &health_binding_view(inputs, published.status_label() == "Ready"),
    ));
    result.push('\n');
    result.push_str(&secret_dismissals_line(inputs.repo_root));
    result.push_str(&process.trailing);
    result
}
