//! Pure formatting functions for all 10 tool responses.
//!
//! All functions take `&LiveIndex` (or data derived from it) and return `String`.
//! No I/O, no async. Output matches the locked formats defined in CONTEXT.md.

// Feature 020 V11 claim attribution (T043). The frozen contract places the file
// at `src/protocol/claim_provenance.rs`, and that is where it lives. The
// DECLARATION is here, in `format.rs`, and deliberately NOT in
// `src/protocol/mod.rs`:
//
//   * `protocol/mod.rs` is inside the `publication_roots` set of the frozen
//     retirement census, so adding a `mod` line there is a release-compiled edit
//     of a hashed file and would move that digest. `format.rs` is in none of the
//     five closure path lists.
//   * `protocol/mod.rs` declares `read_gate` as `pub(crate) mod`, so anchoring
//     there would make the whole provenance module crate-private — and
//     `tests/claim_provenance_v11.rs` is a SEPARATE CRATE that could never see
//     it. `format.rs` is `pub mod`, so the types are reachable from the oracles.
//
// `#[path]` resolves relative to the directory CONTAINING this file, so the bare
// name lands on `src/protocol/claim_provenance.rs`. Verified against rustc
// rather than recalled: `#[path = "../claim_provenance.rs"]` makes it look for
// `src/claim_provenance.rs` and fail.
//
// Do NOT "tidy" this into `protocol/mod.rs`. It silently moves a frozen digest
// and breaks the oracles' visibility in one edit.
#[path = "claim_provenance.rs"]
pub mod claim_provenance;

pub use crate::index_lifecycle::guidance::reference_read::OutputLimits;

use crate::domain::index::{AdmissionTier, SkipReason, SkippedFile};
use crate::live_index::query::{
    EXPECTED_FRAMEWORK_PARTIAL_PARSE_REASON, EXPECTED_GENERATED_PARTIAL_PARSE_REASON,
    EXPECTED_LANGUAGE_PARTIAL_PARSE_REASON, EXPECTED_TEMPLATE_DSL_PARTIAL_PARSE_REASON,
    EXPECTED_TEST_FIXTURE_PARTIAL_PARSE_REASON, EXPECTED_VENDOR_PARTIAL_PARSE_REASON,
};
#[cfg(test)]
use crate::live_index::{
    ContextBundleFoundView, ContextBundleReferenceView, ContextBundleSectionView,
    ContextBundleView, FindReferencesView, ImplBlockSuggestionView, TypeDependencyView,
};
use crate::live_index::{
    FileOutlineView, HealthStats, IndexLoadSource, IndexedFile, LiveIndex, PublishedIndexState,
    RepoOutlineFileView, RepoOutlineView, SnapshotVerifyState, search,
};
use crate::protocol::surface_probe::{SurfaceProfile, connection_surface_or_env};
use crate::{cli::hook::HookAdoptionSnapshot, sidecar::StatsSnapshot};

const PARSE_QUARANTINE_ENTRY_LIMIT: usize = 10;

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
enum ParseQuarantineKind {
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
struct ParseQuarantineEntry {
    path: String,
    kind: ParseQuarantineKind,
    reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ParseQuarantineSummary {
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
}

impl RuntimeMode {
    fn label(self) -> &'static str {
        match self {
            Self::LocalProcess => "local_process",
            Self::DaemonReusedSession => "daemon_reused_session",
            Self::DaemonDegradedLocalFallback => "daemon_degraded_local_fallback",
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

fn index_load_source_label(load_source: IndexLoadSource) -> &'static str {
    load_source.label()
}

fn index_state_label(status: &RuntimeStatus) -> &'static str {
    if current_project_was_reset(status) {
        return "index_folder_reset";
    }

    match status.load_source {
        IndexLoadSource::EmptyBootstrap => "empty_bootstrap",
        IndexLoadSource::FreshLoad => "fresh_process",
        IndexLoadSource::SnapshotRestore => "snapshot_loaded_reused",
    }
}

fn current_project_was_reset(status: &RuntimeStatus) -> bool {
    status.project_generation > 0
        && status.reset_project_generation == Some(status.project_generation)
}

fn reset_state_label(status: &RuntimeStatus) -> String {
    match status.reset_project_generation {
        Some(generation) if generation == status.project_generation => {
            format!("current_project:p{generation}")
        }
        Some(generation) => format!("previous_project:p{generation}"),
        None => "none".to_string(),
    }
}

fn snapshot_verify_state_label(state: &SnapshotVerifyState) -> &'static str {
    match state {
        SnapshotVerifyState::NotNeeded => "not_needed",
        SnapshotVerifyState::Pending => "pending",
        SnapshotVerifyState::Running(_) => "running",
        SnapshotVerifyState::Completed(_) => "completed",
        SnapshotVerifyState::Failed(_) => "failed",
    }
}

fn append_snapshot_verify_mismatch_summary(
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
fn quoted_status_value(value: &str) -> String {
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

fn snapshot_verify_health_line(published: &PublishedIndexState) -> Option<String> {
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

fn snapshot_verify_compact_line(published: &PublishedIndexState) -> Option<String> {
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

fn index_identity(status: &RuntimeStatus) -> String {
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
const RUNTIME_BINARY_VERSION: &str = env!("CARGO_PKG_VERSION");

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

pub fn capability_evidence_line(evidence: &crate::capability::CapabilityEvidence) -> String {
    let mut line = format!("Capability: {} {}", evidence.capability, evidence.status);
    if let Some(detail) = evidence.detail.as_deref().map(str::trim)
        && !detail.is_empty()
    {
        let detail = detail.trim_end_matches('.');
        line.push_str(" - ");
        line.push_str(detail);
    }
    line.push('.');
    line
}

/// Format the file outline for a given path.
///
/// Header: `{path}  ({N} symbols)`
/// Body: each symbol indented by `depth * 2` spaces, then `{kind:<12} {name:<30} {start}-{end}`
/// Not-found: "File not found: {path}"
pub fn file_outline(index: &LiveIndex, path: &str) -> String {
    match index.capture_shared_file(path) {
        Some(file) => file_outline_from_indexed_file(file.as_ref()),
        None => not_found_file(path),
    }
}

pub fn file_outline_from_indexed_file(file: &IndexedFile) -> String {
    render_file_outline(&file.relative_path, &file.symbols)
}

fn render_file_outline(relative_path: &str, symbols: &[crate::domain::SymbolRecord]) -> String {
    let mut lines = Vec::new();
    lines.push(format!("{}  ({} symbols)", relative_path, symbols.len()));

    for sym in symbols {
        let indent = "  ".repeat(sym.depth as usize);
        let kind_str = sym.kind.to_string();
        lines.push(format!(
            "{}{:<12} {:<30} {}-{}",
            indent,
            kind_str,
            sym.name,
            sym.line_range.0 + 1,
            sym.line_range.1 + 1
        ));
    }

    lines.join("\n")
}

pub fn collapse_large_test_modules(
    text: String,
    include_tests: bool,
    sections: Option<&[String]>,
) -> String {
    crate::index_lifecycle::guidance::read_context::collapse_large_test_modules(
        text,
        include_tests,
        sections,
    )
}
/// Compatibility renderer for `FileOutlineView`.
///
/// Main hot-path readers should prefer `file_outline_from_indexed_file()`.
pub fn file_outline_view(view: &FileOutlineView) -> String {
    render_file_outline(&view.relative_path, &view.symbols)
}

pub use crate::index_lifecycle::guidance::symbol_read::inspect_match_result_view;
#[cfg(test)]
use crate::index_lifecycle::guidance::symbol_read::symbol_kind_name_label;
pub use crate::index_lifecycle::guidance::symbol_read::{
    code_slice_from_indexed_file, code_slice_view, symbol_detail, symbol_detail_from_indexed_file,
    symbol_detail_view,
};

/// Search for symbols matching a query (case-insensitive), with 3-tier scored ranking.
///
/// Output sections (only non-empty tiers shown):
/// ```text
/// ── Exact matches ──
///   {line}: {kind} {name}  ({file})
///
/// ── Prefix matches ──
///   ...
///
/// ── Substring matches ──
///   ...
/// ```
/// Header: `{N} matches in {M} files`
/// Empty: "No symbols matching '{query}'"
pub fn search_symbols_result(index: &LiveIndex, query: &str) -> String {
    search_symbols_result_with_kind(index, query, None)
}

pub fn search_symbols_result_with_kind(
    index: &LiveIndex,
    query: &str,
    kind_filter: Option<&str>,
) -> String {
    let result = search::search_symbols(
        index,
        query,
        kind_filter,
        search::ResultLimit::symbol_search_default().get(),
    );
    search_symbols_result_view(&result, query)
}

pub use crate::index_lifecycle::guidance::search_render::search_symbols_result_view;

/// Search for text content matches (case-insensitive substring).
///
/// For queries >= 3 chars, uses the TrigramIndex to select candidate files before scanning.
/// For queries < 3 chars, falls back to scanning all files (trigram search handles this internally).
///
/// Header: `{N} matches in {M} files`
/// Body: grouped by file, each match: `  {line_number}: {line_content}`
/// Empty: "No matches for '{query}'"
pub fn search_text_result(index: &LiveIndex, query: &str) -> String {
    search_text_result_with_options(index, Some(query), None, false)
}

pub fn search_text_result_with_options(
    index: &LiveIndex,
    query: Option<&str>,
    terms: Option<&[String]>,
    regex: bool,
) -> String {
    let result = search::search_text(index, query, terms, regex);
    search_text_result_view(
        result,
        None,
        None,
        None,
        SearchSuggestionContext {
            regex,
            include_tests: false,
            multi_word_literal: !regex
                && query.is_some_and(|q| q.trim().contains(char::is_whitespace)),
        },
    )
}

pub(crate) use crate::index_lifecycle::guidance::search_render::append_excluded_knowledge_note;
pub use crate::index_lifecycle::guidance::search_render::{
    SearchSuggestionContext, search_text_result_view,
};

/// Generate a depth-limited source file tree with symbol counts per file and directory.
///
/// - `path`: subtree prefix filter (empty/blank = project root).
/// - `depth`: maximum depth levels to expand (default 2, max 5).
///
/// Output format:
/// ```text
/// {dir}/  ({N} files, {M} symbols)
///   {file} [{lang}]  ({K} symbols)
///   {subdir}/  ({N} files, {M} symbols)
/// ...
/// {D} directories, {F} files, {S} symbols
/// ```
pub fn file_tree(index: &LiveIndex, path: &str, depth: u32) -> String {
    let view = index.capture_repo_outline_view();
    file_tree_view(&view.files, path, depth)
}

pub fn file_tree_view(files: &[RepoOutlineFileView], path: &str, depth: u32) -> String {
    crate::index_lifecycle::guidance::read_context::file_tree_view(files, path, depth)
}

pub fn file_tree_view_with_skipped(
    files: &[RepoOutlineFileView],
    skipped: &[SkippedFile],
    path: &str,
    depth: u32,
) -> String {
    crate::index_lifecycle::guidance::read_context::file_tree_view_with_skipped(
        files, skipped, path, depth,
    )
}
/// Generate a directory-tree overview of the repo.
///
/// Header: `{project_name}  ({N} files, {M} symbols)`
/// Body: sorted paths, each: `  {filename:<20} {language:<12} {symbol_count} symbols`
pub fn repo_outline(index: &LiveIndex, project_name: &str) -> String {
    let view = index.capture_repo_outline_view();
    repo_outline_view(&view, project_name)
}

pub fn repo_outline_view(view: &RepoOutlineView, project_name: &str) -> String {
    crate::index_lifecycle::guidance::read_context::repo_outline_view(view, project_name)
}
/// Generate a health report for the index.
///
/// Watcher state is read from `health_stats()` (Off defaults when no watcher is active).
/// Use `health_report_with_watcher` when the live `WatcherInfo` should be reflected.
///
/// Format:
/// ```text
/// Status: {Ready|Empty|Degraded}
/// Files:  {N} indexed ({P} parsed, {PP} partial, {F} failed)
/// Symbols: {S}
/// Loaded in: {D}ms
/// Watcher: active ({E} events, last: {T}, debounce: {D}ms)
///     or: degraded ({E} events processed before failure)
///     or: off
/// ```
pub fn health_report(index: &LiveIndex) -> String {
    use crate::live_index::IndexState;

    let state = index.index_state();
    let status = match state {
        IndexState::Empty => "Empty",
        IndexState::Ready => "Ready",
        IndexState::Loading => "Loading",
        IndexState::CircuitBreakerTripped { .. } => "Degraded",
    };
    let stats = index.health_stats();
    health_report_from_stats(status, &stats, 0)
}

/// Generate a health report for the index with live watcher state.
///
/// Uses `health_stats_with_watcher` to incorporate the live `WatcherInfo` into the report.
/// Called by the `health` tool handler in production (watcher is always available there).
pub fn health_report_with_watcher(
    index: &LiveIndex,
    watcher: &crate::watcher::WatcherInfo,
) -> String {
    use crate::live_index::IndexState;

    let state = index.index_state();
    let status = match state {
        IndexState::Empty => "Empty",
        IndexState::Ready => "Ready",
        IndexState::Loading => "Loading",
        IndexState::CircuitBreakerTripped { .. } => "Degraded",
    };
    let stats = index.health_stats_with_watcher(watcher);
    health_report_from_stats(status, &stats, 0)
}

pub fn health_report_from_published_state(
    published: &PublishedIndexState,
    watcher: &crate::watcher::WatcherInfo,
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
    watcher: &crate::watcher::WatcherInfo,
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
    if matches!(stats.watcher_state, crate::watcher::WatcherState::Off) {
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
    watcher: &crate::watcher::WatcherInfo,
    rejected_stale_mutations: u64,
) -> String {
    use crate::watcher::WatcherState;

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
fn short_digest(value: &str) -> &str {
    // Char-boundary safe: digests/source-ids are ASCII hex today, but the
    // helper takes an arbitrary &str, so never byte-slice mid-codepoint.
    match value.char_indices().nth(12) {
        Some((idx, _)) => &value[..idx],
        None => value,
    }
}

fn coverage_label(coverage: CoverageStatus) -> &'static str {
    match coverage {
        CoverageStatus::Complete => "complete",
        CoverageStatus::Degraded => "degraded",
    }
}

fn placement_label(placement: Option<&StatePlacement>) -> &'static str {
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

fn authorization_label(placement: Option<&StatePlacement>) -> &'static str {
    match placement_authorization(placement) {
        Some(SourceAccessMode::NormalProject) => "normal",
        Some(SourceAccessMode::ExplicitProtected) => "explicit_protected",
        // Unbound only: no source is authorized, so the closed bound-source set
        // does not apply.
        None => "not_applicable",
    }
}

fn capability_unavailable_code(reason: CapabilityUnavailableReason) -> &'static str {
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
fn persistence_label(status: CapabilityStatus) -> String {
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

fn replay_label(placement: Option<&StatePlacement>, persistence: CapabilityStatus) -> &'static str {
    match (placement, persistence) {
        (
            Some(StatePlacement::ProjectLocal { .. } | StatePlacement::UserLocal { .. }),
            CapabilityStatus::Available,
        ) => "available",
        _ => "unavailable",
    }
}

fn render_source_binding_line(binding: &SourceBindingHealthView<'_>) -> String {
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
fn manifest_coverage_digest(manifest: Option<&RepositoryManifest>) -> String {
    match manifest {
        Some(manifest) => format!(
            "coverage={} digest={}",
            coverage_label(manifest.coverage),
            short_digest(&manifest.digest),
        ),
        None => "coverage=absent digest=none".to_string(),
    }
}

fn freshness_label(freshness: &FreshnessStatus) -> String {
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
fn render_manifest_line(
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
fn render_disposition_line(entries: &[CatalogEntry]) -> String {
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
fn render_source_entry_line(
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

const MAX_HEALTH_SOURCES: usize = 8;

fn render_source_set_section(source_set: &PublishedSourceSet) -> String {
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

fn derived_coverage_label(coverage: &DerivedCoverage) -> String {
    match coverage {
        DerivedCoverage::Complete => "complete".to_string(),
        DerivedCoverage::Truncated { breaches } => {
            let omitted: u64 = breaches.iter().map(|breach| breach.omitted).sum();
            format!("truncated({omitted} omitted)")
        }
        DerivedCoverage::Loading => "loading".to_string(),
    }
}

fn render_bridge_line(bridge: &KnowledgeBridge, content_generation: u64) -> String {
    format!(
        "Bridge: coverage={} cards={} forward_links={} knowledge_links={} version=content{}",
        derived_coverage_label(&bridge.coverage),
        bridge.cards.len(),
        bridge.forward.len(),
        bridge.knowledge_links.len(),
        content_generation,
    )
}

fn render_temporal_line(code_signals: &CodeSignalsSnapshot) -> String {
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

fn policy_status_label(status: &PolicyLedgerStatus) -> String {
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

fn render_authority_hygiene_line(authority: &KnowledgeAuthorityView) -> String {
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

fn sidecar_pid_label(status: &crate::sidecar::port_file::SidecarStatus) -> String {
    status
        .pid
        .map(|pid| pid.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn sidecar_port_label(status: &crate::sidecar::port_file::SidecarStatus) -> String {
    status
        .port
        .map(|port| port.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

pub(crate) fn format_sidecar_status(status: &crate::sidecar::port_file::SidecarStatus) -> String {
    if status.liveness == crate::sidecar::port_file::SidecarLiveness::NoSidecar {
        // Identity-rejected candidates must NOT vanish into a bare "none":
        // rejected descriptors mean a sidecar EXISTS but refused this
        // caller's root — exactly the evidence a wrong-project investigation
        // needs.
        return match status.detail.as_deref() {
            Some(detail) => format!("Sidecar: none usable ({detail})"),
            None => "Sidecar: none (.symforge/sidecar.* absent)".to_string(),
        };
    }

    let mut line = format!(
        "Sidecar: pid={} port={} state={}",
        sidecar_pid_label(status),
        sidecar_port_label(status),
        status.liveness.as_str(),
    );
    if let Some(detail) = status.detail.as_deref() {
        line.push_str(&format!(" ({detail})"));
    }
    line
}

pub(crate) fn format_sidecar_status_compact(
    status: &crate::sidecar::port_file::SidecarStatus,
) -> String {
    if status.liveness == crate::sidecar::port_file::SidecarLiveness::NoSidecar {
        return match status.detail.as_deref() {
            Some(detail) => format!("Sidecar: none usable ({detail})"),
            None => "Sidecar: none".to_string(),
        };
    }

    format!(
        "Sidecar: {} pid={} port={}",
        status.liveness.as_str(),
        sidecar_pid_label(status),
        sidecar_port_label(status),
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
    use crate::watcher::WatcherState;

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

fn is_local_fallback_stats(stats: &HealthStats) -> bool {
    matches!(stats.watcher_state, crate::watcher::WatcherState::Off)
        && stats.events_processed == 0
        && stats.last_event_at.is_none()
        && stats.debounce_window_ms == 0
        && stats.overflow_count == 0
        && stats.last_overflow_at.is_none()
        && stats.stale_files_found == 0
        && stats.last_reconcile_at.is_none()
}

/// List files changed since the given Unix timestamp.
///
/// If since_ts < loaded_at: return list of all files (entire index is "newer")
/// If since_ts >= loaded_at: return "No changes detected since last index load."
pub fn what_changed_result(index: &LiveIndex, since_ts: i64) -> String {
    let view = index.capture_what_changed_timestamp_view();
    what_changed_timestamp_view(&view, since_ts)
}

/// Render `detect_impact`'s JSON payload wrapped in a short plain-text
/// summary. MCP tool responses here are always text, never raw JSON (house
/// convention — see `tools.rs` module doc); the exact contract shape
/// (contracts/detect-impact.md § Output) is embedded verbatim after the
/// `--- impact payload ---` marker so callers can parse it directly, the same
/// pattern `format_session_cache_hit_body` uses for its cache payload.
pub fn detect_impact_result(
    payload: &serde_json::Value,
    requested_depth: u8,
    effective_depth: u8,
    base_ref: Option<&str>,
    staleness_note: Option<&str>,
) -> String {
    // Counts come from the per-list `pagination` totals, NOT the (capped) arrays,
    // so the summary reports the FULL change/blast size even when the lists are
    // truncated (Wave 1 Fix 1).
    let pagination = &payload["pagination"];
    let changed_files = pagination["changed_files"]["total"].as_u64().unwrap_or(0);
    let changed_symbols = pagination["changed_symbols"]["total"].as_u64().unwrap_or(0);
    let total_blast = pagination["blast_radius"]["total"].as_u64().unwrap_or(0);
    let risk = &payload["risk_summary"];
    let mut summary = format!(
        "Impact analysis: {changed_files} changed file(s), {changed_symbols} changed symbol(s), \
         {total_blast} blast-radius node(s) ({} critical / {} high / {} medium / {} low)",
        risk["critical"], risk["high"], risk["medium"], risk["low"],
    );
    // Self-describing base ref + staleness disclosure (Wave 1 Fix 6).
    if let Some(base) = base_ref {
        summary.push_str(&format!("\nbase: {base}"));
    }
    if let Some(note) = staleness_note {
        summary.push_str(&format!("\nnote: {note}"));
    }
    // Truncation disclosure in the human summary (machine-readable totals live in
    // `pagination`), using the house truncation marker (Wave 1 Fix 1).
    let any_truncated = ["changed_files", "changed_symbols", "blast_radius"]
        .iter()
        .any(|list| pagination[*list]["truncated"].as_bool().unwrap_or(false));
    if any_truncated {
        summary.push_str(&format!(
            "\n{CANONICAL_TRUNCATION_MARKER} one or more lists capped; see `pagination` for full totals and returned counts."
        ));
    }
    let json = serde_json::to_string_pretty(payload).expect("detect_impact payload serializes");
    let mut out = format!("{summary}\n\n--- impact payload ---\n{json}");
    if requested_depth > effective_depth {
        out.push_str(&format!(
            "\n\nWarning: depth clamped to {effective_depth} (requested {requested_depth})."
        ));
    }
    out
}

pub(crate) use crate::index_lifecycle::guidance::changes::{
    what_changed_paths_result, what_changed_timestamp_view,
};
/// Fix 3 (Wave 1): cap the changed/uncommitted-path listing. On a large repo the
/// working-tree listing reached ~100 KB of raw paths; bound it and disclose the
/// omitted count with the house truncation marker.
pub use crate::index_lifecycle::guidance::search_render::search_files_resolve_result_view;

pub fn search_files(index: &LiveIndex, query: &str, limit: usize) -> String {
    let view = index.capture_search_files_view(query, limit, None, None);
    search_files_result_view(&view)
}

pub use crate::index_lifecycle::guidance::search_render::search_files_result_view;

pub(crate) use crate::index_lifecycle::guidance::file_read::render_file_content_bytes;
pub use crate::index_lifecycle::guidance::file_read::{
    GET_FILE_CONTENT_MAX_BYTES, append_nul_byte_warning, cap_file_content_output, file_content,
    file_content_from_indexed_file, file_content_from_indexed_file_with_context, file_content_view,
    not_found_file, not_found_file_match, not_found_symbol,
};

pub fn validate_file_syntax_result(path: &str, file: &IndexedFile) -> String {
    let mut lines = vec![
        format!("Syntax validation: {path}"),
        format!("Language: {}", file.language),
    ];

    match &file.parse_status {
        crate::live_index::ParseStatus::Parsed => {
            lines.push("Status: ok".to_string());
        }
        crate::live_index::ParseStatus::PartialParse { warning } => {
            // SF-003: a TypeScript import-type immediately followed by `[]`
            // (e.g. `import('rxjs').Subscription[]`) is mis-parsed by
            // tree-sitter-typescript 0.23.2 even though it is valid TS. When the
            // partial parse is provably caused only by this grammar limitation,
            // report it as ok with an explanatory note rather than a syntax
            // error.
            if crate::parsing::is_expected_typescript_import_type_array_limitation(
                &file.language,
                &file.content,
                crate::domain::LanguageId::is_tsx_path(&file.relative_path),
            ) {
                lines.push("Status: ok".to_string());
                lines.push(
                    "Note: parser limitation (tree-sitter-typescript 0.23.2 mis-parses an \
                     import-type followed by `[]`; the source is valid TypeScript)"
                        .to_string(),
                );
            } else {
                lines.push("Status: partial".to_string());
                if let Some(diagnostic) = &file.parse_diagnostic {
                    lines.push(format!("Diagnostic: {}", diagnostic.summary()));
                    if let Some((start, end)) = diagnostic.byte_span {
                        lines.push(format!("Byte span: {start}..{end}"));
                    }
                } else {
                    lines.push(format!("Diagnostic: {warning}"));
                }
            }
        }
        crate::live_index::ParseStatus::Failed { error } => {
            lines.push("Status: failed".to_string());
            if let Some(diagnostic) = &file.parse_diagnostic {
                lines.push(format!("Diagnostic: {}", diagnostic.summary()));
                if let Some((start, end)) = diagnostic.byte_span {
                    lines.push(format!("Byte span: {start}..{end}"));
                }
            } else {
                lines.push(format!("Diagnostic: {error}"));
            }
        }
    }

    lines.push(format!("Symbols extracted: {}", file.symbols.len()));
    lines.join("\n")
}

/// Explicit "outside the repository root" error for a path that escapes the
/// indexed repo (e.g. `../../../etc/passwd`). Distinct from `not_found_file` so a
/// traversal attempt is reported as a containment violation rather than a
/// misleading generic miss.
pub fn path_outside_repo(path: &str) -> String {
    format!(
        "Path is outside the repository root: {path}. \
         get_file_content only reads files within the indexed repository; \
         supply a repo-relative path."
    )
}

/// Refusal for a file a secret-detector CONTENT rule excluded from disclosure.
///
/// Names the rule ids, the finding count and, when the gate could compute them,
/// the 1-based finding lines — never a byte of the file — so a false positive
/// can be located and reported (owner ruling 2026-09-29; an earlier revision
/// named nothing, which left every false positive undiagnosable). The refusal
/// text itself offers no self-service bypass; actionable remediation is a
/// separate write tool (`secret_remediate`) reached via `_meta["symforge/withheld"]`
/// (feature 034). A reindex still gives the same verdict on the same bytes.
pub use crate::index_lifecycle::guidance::read_admission_format::content_withheld_by_admission;

/// Refusal for a file a PATH rule excluded: a credential container by name
/// (`.env`, private keys, cloud credential stores). Names the rule.
pub use crate::index_lifecycle::guidance::read_admission_format::content_withheld_by_path_rule;

/// Refusal for a file the admission pipeline could not INSPECT at all — over the
/// deterministic scan budget, or not decodable as searchable text.
///
/// Shares [`content_withheld_by_admission`]'s opening clause, so one anchored
/// predicate classifies both and every contract keyed on that prefix keeps
/// holding. Differs in what follows: these files were never inspected, so
/// there is no rule to name, and they refuse the same way on every read —
/// reindexing will not change them. Names no size, no threshold, no encoding, no rule id, no
/// finding count, and no byte; the wording is identical for both causes, so it
/// does not even distinguish which one applied.
pub use crate::index_lifecycle::guidance::read_admission_format::content_withheld_unscanned;

/// Refusal for a restored file the snapshot verify could not reconcile. Its
/// restored content is withheld rather than served as current, and the file
/// is not reported absent, because it is not.
pub use crate::index_lifecycle::guidance::read_admission_format::unverified_since_restore;

/// Paths the snapshot verify withheld inside `scope`, bounded for display:
/// the count, up to `limit` paths, and how many more there are.
use crate::index_lifecycle::guidance::read_context::withheld_listing;

/// Appended to a project-wide answer when the snapshot verify withheld files
/// inside its scope: those files were not searched, so the answer may be
/// missing hits from them.
pub fn withheld_not_searched_note(
    unverified: &std::collections::BTreeMap<String, String>,
    scope: Option<&str>,
) -> Option<String> {
    crate::index_lifecycle::guidance::read_context::withheld_not_searched_note(unverified, scope)
}
/// A project-wide mutation cannot see references inside withheld files, so
/// it refuses rather than leave them half-renamed.
pub fn project_wide_mutation_refused_unverified(
    tool: &str,
    unverified: &std::collections::BTreeMap<String, String>,
) -> Option<String> {
    let (count, listed) = withheld_listing(unverified, None, 10)?;
    Some(format!(
        "{tool} refused: {count} files are withheld as unverified since restore, and a \
         project-wide change cannot see what they contain: {listed}. A successful re-read \
         releases each one; run index_folder to rebuild the project from source, then retry."
    ))
}

/// Richer "file not found" with suggested similar paths.
/// Call this from tool handlers where the index is available.
pub fn not_found_file_with_suggestions(path: &str, suggestions: &[String]) -> String {
    if suggestions.is_empty() {
        format!("File not found: {path}. Use search_files to find the correct path.")
    } else {
        let top: Vec<&str> = suggestions.iter().take(5).map(|s| s.as_str()).collect();
        format!("File not found: {path}. Did you mean: {}?", top.join(", "))
    }
}

/// Honest "not indexed" response for a path that EXISTS but was deliberately
/// admitted as Tier-2 (metadata-only) or Tier-3 (hard-skipped) rather than
/// parsed. Distinct from `not_found_file`: the file is present on disk, so a
/// bare "File not found" is wrong and confusing. This message names the tier,
/// the skip reason, and the size.
///
/// The recovery sentence is keyed on `raw_read_available` — whether the
/// spec-023 read gate would PERMIT a `get_file_content` raw read of this path.
/// Advising "Use get_file_content" for a path the gate refuses sends the
/// caller into a guaranteed refusal loop (the Tier-2 UX contradiction,
/// testpilot receipt 2026-08-03), so a gated path gets the honest withheld
/// sentence instead.
///
/// Example (Tier 2, raw read available):
///   `Not indexed: package-lock.json is Tier 2 (metadata only) — reason:
///    lockfile, size 406 KB. Use get_file_content for raw reads if you need
///    the contents.`
pub fn not_indexed_skipped_file(
    path: &str,
    tier: AdmissionTier,
    reason: Option<SkipReason>,
    size: Option<u64>,
    raw_read_available: bool,
) -> String {
    let reason_str = reason
        .map(|r| r.to_string())
        .unwrap_or_else(|| "skipped".to_string());
    let size_str = size
        .map(crate::index_lifecycle::guidance::read_context::human_size)
        .unwrap_or_else(|| "unknown".to_string());
    match tier {
        AdmissionTier::Normal => {
            // Not a skip; callers should never reach here with a Normal tier,
            // but degrade gracefully rather than assert.
            not_found_file(path)
        }
        AdmissionTier::MetadataOnly if raw_read_available => format!(
            "Not indexed: {path} is Tier 2 (metadata only) — reason: {reason_str}, size {size_str}. \
             Use get_file_content for raw reads if you need the contents."
        ),
        AdmissionTier::MetadataOnly => format!(
            "Not indexed: {path} is Tier 2 (metadata only) — reason: {reason_str}, size {size_str}. \
             Its contents are withheld by the admission policy — get_file_content will refuse this \
             file, so read it outside SymForge if you need the raw text."
        ),
        AdmissionTier::HardSkip if raw_read_available => format!(
            "Not indexed: {path} is Tier 3 (hard-skipped) — reason: {reason_str}, size {size_str}. \
             This file is not parsed or read; get_file_content may still read it raw if it is a \
             text file within the repository."
        ),
        AdmissionTier::HardSkip => format!(
            "Not indexed: {path} is Tier 3 (hard-skipped) — reason: {reason_str}, size {size_str}. \
             This file is not parsed or read, and its contents are withheld by the admission \
             policy — get_file_content will refuse this file, so read it outside SymForge if you \
             need the raw text."
        ),
    }
}

/// Simple edit-distance score for fuzzy matching (lower is closer).
pub use crate::index_lifecycle::guidance::reference_read::{
    find_dependents_compact_view, find_dependents_dot, find_dependents_mermaid,
    find_dependents_result, find_dependents_result_view, find_references_compact_view,
    find_references_result, find_references_result_view, implementations_result_view,
};

use crate::index_lifecycle::guidance::source::CANONICAL_TRUNCATION_MARKER;
#[cfg(test)]
use crate::index_lifecycle::guidance::source::extract_signature;
#[cfg(test)]
use crate::index_lifecycle::guidance::symbol_context::{
    apply_verbosity, auto_summarize, format_type_dependencies, heuristic_from_name,
};
pub use crate::index_lifecycle::guidance::symbol_context::{
    context_bundle_callees_text, context_bundle_impl_suggestion_tip, context_bundle_result,
    context_bundle_result_view, context_bundle_result_view_with_max_tokens,
    trace_symbol_result_view,
};

pub use crate::index_lifecycle::guidance::source::{
    downgrade_full_completeness_after_truncation, enforce_token_budget,
    enforce_token_budget_flagged,
};

/// The loading-guard refusal: "Index is loading...", then the indexing
/// wording. It says "initial" only for a cold bootstrap, the one load source
/// known to be a project's first index; a warm restore still verifying or a
/// reload in flight is loading too, but is not initial indexing.
pub fn loading_guard_message(cold_bootstrap: bool) -> String {
    let wording = if cold_bootstrap {
        INITIAL_INDEXING_IN_PROGRESS
    } else {
        INDEXING_IN_PROGRESS
    };
    format!("Index is loading... {wording}. Retry the same call shortly.")
}

/// The single wording for a project whose index is loading, said wherever the
/// kind of load is not known: the stdio front's not-ready answer, `status` and
/// `health` in that window, and the SessionStart hook. It claims no waiting:
/// only the stdio front waits, and says so itself.
pub const INDEXING_IN_PROGRESS: &str =
    "indexing of this project is in progress and can take a while on large folders";

/// [`INDEXING_IN_PROGRESS`] for a load known to be a cold bootstrap.
pub const INITIAL_INDEXING_IN_PROGRESS: &str =
    "initial indexing of this project is in progress and can take a while on large folders";

/// Loading-guard prefix while a restored snapshot is being verified. The
/// `Index is loading` start is load-bearing: `is_index_unavailable_output` and
/// `edit_output_is_error` classify the guard by it.
pub const SNAPSHOT_VERIFY_IN_PROGRESS: &str =
    "Index is loading: verifying a restored snapshot against disk";

/// Loading-guard text that names what holds the index. A restored snapshot
/// stays unqueryable until its verification completes (Feature 020 V11: only a
/// complete verified generation is queryable), so say which phase it is in and
/// how far it got instead of a bare "try again shortly".
pub fn loading_guard_message_for(state: &SnapshotVerifyState, cold_bootstrap: bool) -> String {
    let detail = match state {
        SnapshotVerifyState::Pending => "verify=pending".to_string(),
        SnapshotVerifyState::Running(progress) => {
            format!("verify=running {}", progress.describe())
        }
        SnapshotVerifyState::NotNeeded
        | SnapshotVerifyState::Completed(_)
        | SnapshotVerifyState::Failed(_) => {
            return loading_guard_message(cold_bootstrap);
        }
    };
    format!(
        "{SNAPSHOT_VERIFY_IN_PROGRESS} ({detail}). It re-reads the files that changed \
         since the snapshot and publishes them once; restored files stay hidden until it \
         completes. `status` and `health` report its progress."
    )
}

/// Surface-aware empty-index recovery hint (TR-02 / N-5 / FR-011, FR-012).
///
/// Every empty-index / "not loaded" error an agent can reach must name ONLY a
/// recovery action that is callable on the agent's *active* surface. The compact
/// surface forbids `index_folder` (it is not one of the compact-3 tools); naming
/// it there sends the agent into an unrecoverable loop — told to call a tool its
/// surface rejects at dispatch. This single function is the one source of truth
/// for that message, computed from the **active** [`SurfaceProfile`], never a
/// fixed string.
///
/// - [`SurfaceProfile::Compact`]: names only operator-actionable recovery that
///   actually works from a cold / home-cwd start — set `SYMFORGE_WORKSPACE_ROOT`
///   or run `symforge init`, then reconnect — plus the documented
///   `SYMFORGE_SURFACE=full` opt-out. MUST NOT mention `index_folder` (gated),
///   and MUST NOT tell an LLM to "re-launch" (it cannot relaunch its own server).
///   The `Index not loaded.` prefix is load-bearing — empty-index classifiers
///   in tools/edit/golden-replay detect the guard by it.
/// - [`SurfaceProfile::Full`] / [`SurfaceProfile::Meta`]: `index_folder` is
///   callable, so the hint may name it directly.
pub fn empty_index_recovery_hint(profile: SurfaceProfile) -> String {
    match profile {
        SurfaceProfile::Compact => "Index not loaded. SymForge has no resolvable project \
             root. To recover, set SYMFORGE_WORKSPACE_ROOT to the project path in this client's \
             MCP config, or run `symforge init` for this harness, then reconnect. \
             (SYMFORGE_SURFACE=full exposes the full tool surface for manual indexing.)"
            .to_string(),
        SurfaceProfile::Full | SurfaceProfile::Meta => {
            "Index not loaded. Call index_folder to index a directory.".to_string()
        }
    }
}

/// Empty-index guard message computed from the surface served on THIS
/// connection.
///
/// Thin wrapper over [`empty_index_recovery_hint`] that resolves the active
/// surface via [`connection_surface_or_env`]. Existing guard sites and the
/// `loading_guard!` macro call this so the emitted message is always surface-
/// aware without each site having to thread the profile by hand (N-5: one
/// function, no residual hardcoded "Call index_folder" on a compact path).
///
/// D23: `connection_surface_or_env` reads the proxied connection surface when a
/// daemon-side proxied tool call is in flight (bound from the adapter's
/// `CONNECTION_SURFACE_HEADER`), else this process's env. This closes the
/// adapter/daemon env-skew hole where a compact adapter proxied through a full-
/// env daemon was handed a hint naming the gated `index_folder`.
pub fn empty_guard_message() -> String {
    empty_index_recovery_hint(connection_surface_or_env())
}

/// Format a "Token Savings (this session)" section from a `StatsSnapshot`.
///
/// Input: `snap` — the `StatsSnapshot` from `TokenStats::summary()`.
/// Output: a multi-line string listing per-hook-type fire counts and token savings.
///
/// If all counters are zero, returns an empty string (no savings section shown).
/// This is a fail-open function — callers can append the result without checking emptiness.
///
/// ```text
/// ── Token Savings (this session) ──
/// Read:  N fires, ~M tokens saved
/// Edit:  N fires, ~M tokens saved
/// Write: N fires
/// Grep:  N fires, ~M tokens saved
/// Total: ~T tokens saved
/// ```
pub fn format_token_savings(snap: &StatsSnapshot) -> String {
    let total_saved = snap.read_saved_tokens + snap.edit_saved_tokens + snap.grep_saved_tokens;

    // Show section only when at least one hook has fired.
    let any_fires =
        snap.read_fires > 0 || snap.edit_fires > 0 || snap.write_fires > 0 || snap.grep_fires > 0;

    if !any_fires {
        return String::new();
    }

    let mut lines = vec!["── Token Savings (this session) ──".to_string()];

    if snap.read_fires > 0 {
        lines.push(format!(
            "Read:  {} fires, ~{} tokens saved",
            snap.read_fires, snap.read_saved_tokens
        ));
    }
    if snap.edit_fires > 0 {
        lines.push(format!(
            "Edit:  {} fires, ~{} tokens saved",
            snap.edit_fires, snap.edit_saved_tokens
        ));
    }
    if snap.write_fires > 0 {
        lines.push(format!("Write: {} fires", snap.write_fires));
    }
    if snap.grep_fires > 0 {
        lines.push(format!(
            "Grep:  {} fires, ~{} tokens saved",
            snap.grep_fires, snap.grep_saved_tokens
        ));
    }

    lines.push(format!(
        "Total: ~{} tokens saved (vs competent-manual windowed read)",
        total_saved
    ));

    lines.join("\n")
}

/// Format a per-tool token breakdown section showing tokens served, saved, and efficiency ratio.
///
/// Input: `details` — sorted Vec of `(tool_name, tokens_served, tokens_saved)`.
/// Returns empty string when details is empty.
pub fn format_tool_token_breakdown(details: &[(String, u64, u64)]) -> String {
    if details.is_empty() {
        return String::new();
    }

    let total_served: u64 = details.iter().map(|(_, s, _)| s).sum();
    let total_saved: u64 = details.iter().map(|(_, _, s)| s).sum();
    let total_naive = total_served + total_saved;
    let efficiency = if total_served > 0 {
        total_naive as f64 / total_served as f64
    } else {
        1.0
    };
    let reduction_pct = if total_naive > 0 {
        (total_saved as f64 / total_naive as f64 * 100.0) as u64
    } else {
        0
    };

    let mut lines = vec![format!(
        "\u{2500}\u{2500} Session Efficiency (competent-manual baseline) \u{2500}\u{2500}\nTokens served: {}\nCompetent-manual equivalent: {}\nEfficiency: {:.1}x ({reduction_pct}% reduction vs windowed-read baseline)",
        total_served, total_naive, efficiency
    )];

    lines.push(String::new());
    lines.push("\u{2500}\u{2500} Per-Tool Breakdown \u{2500}\u{2500}".to_string());
    let max_name = details.iter().map(|(n, _, _)| n.len()).max().unwrap_or(0);
    for (name, served, saved) in details.iter().take(10) {
        let tool_naive = served + saved;
        let tool_eff = if *served > 0 {
            format!("{:.1}x", tool_naive as f64 / *served as f64)
        } else {
            "-".to_string()
        };
        lines.push(format!(
            "  {:<width$}  {} served, {} saved ({})",
            name,
            served,
            saved,
            tool_eff,
            width = max_name
        ));
    }

    lines.join("\n")
}

/// Format a "Tool Call Counts (this session)" section from per-tool invocation counts.
///
/// Input: `counts` — sorted slice of `(tool_name, count)` from `TokenStats::tool_call_counts()`.
/// Output: a multi-line string. Returns empty string when `counts` is empty.
///
/// ```text
/// ── Tool Call Counts (this session) ──
/// search_text:        12
/// get_file_context:    7
/// get_symbol:          3
/// ```
pub fn format_tool_call_counts(counts: &[(String, usize)]) -> String {
    if counts.is_empty() {
        return String::new();
    }

    let mut lines = vec!["── Tool Call Counts (this session) ──".to_string()];
    // Align counts by padding tool names to the width of the longest name.
    let max_name_len = counts.iter().map(|(n, _)| n.len()).max().unwrap_or(0);
    for (name, count) in counts {
        lines.push(format!("{:<width$}  {}", name, count, width = max_name_len));
    }

    lines.join("\n")
}

/// Competent-manual read baseline: grep-then-~50-line window, calibrated from
/// sf-bench phase-1 token-savings benchmark (2026-06-12, S-vs-M headline tasks).
pub use crate::index_lifecycle::guidance::file_read::COMPETENT_READ_WINDOW_LINES;

/// Whole-file char count below which a competent agent reads the entire file.
pub use crate::index_lifecycle::guidance::file_read::SMALL_FILE_CHAR_THRESHOLD;

/// Line count at or below which responses should stay minimal (outline-only, no hints).
pub use crate::index_lifecycle::guidance::read_context::SMALL_FILE_LINE_THRESHOLD;

/// Whole-file read baseline (legacy naive comparison — kept for transparency).
pub fn whole_file_baseline_chars(raw_chars: usize) -> usize {
    raw_chars
}

/// Windowed read a disciplined agent would do instead of reading the whole file.
pub use crate::index_lifecycle::guidance::file_read::competent_manual_baseline_chars;

pub fn indexed_file_line_count(content: &[u8]) -> usize {
    crate::index_lifecycle::guidance::read_context::indexed_file_line_count(content)
}

pub fn is_small_indexed_file(content_len: usize, line_count: usize) -> bool {
    crate::index_lifecycle::guidance::read_context::is_small_indexed_file(content_len, line_count)
}
/// Default read budget when callers omit `max_tokens` (~50-line competent window).
pub use crate::index_lifecycle::guidance::file_read::default_read_max_tokens;

/// Resolve an explicit or default token budget for read-like tools.
pub use crate::index_lifecycle::guidance::file_read::resolve_read_max_tokens;

/// Estimated tokens from a character count (`~chars/4` approximation).
///
/// Coarse heuristic, NOT a measured token count. Callers that surface the
/// result to an agent label it as an estimate (the savings footer uses `~N
/// tokens` framing) so no figure is presented as exact/measured (010 N-4).
pub use crate::index_lifecycle::guidance::file_read::estimate_tokens_from_chars;

pub fn saved_tokens_whole_file(response_chars: usize, raw_chars: usize) -> u64 {
    crate::index_lifecycle::guidance::read_context::saved_tokens_whole_file(
        response_chars,
        raw_chars,
    )
}

pub fn saved_tokens_vs_competent_manual(response_chars: usize, raw_chars: usize) -> u64 {
    crate::index_lifecycle::guidance::read_context::saved_tokens_vs_competent_manual(
        response_chars,
        raw_chars,
    )
}
/// Baseline for search/reference listing tools without a single raw file length.
pub fn estimate_listing_baseline_chars(output_chars: usize) -> usize {
    let hits = (output_chars / 80).max(1);
    let grep = hits.saturating_mul(120);
    let windows = (hits / 5 + 1).saturating_mul(competent_manual_baseline_chars(4000));
    grep.saturating_add(windows).max(output_chars)
}

/// Estimate tokens saved by a structured response vs raw file content.
/// Returns a one-line footer string, or empty string if no meaningful savings.
pub fn compact_savings_footer(response_chars: usize, raw_chars: usize) -> String {
    crate::index_lifecycle::guidance::read_context::compact_savings_footer(
        response_chars,
        raw_chars,
    )
}

pub use crate::index_lifecycle::guidance::source::format_session_cache_hit_body;

/// Dedup hint when agent forces a re-fetch of content already in session (011 US4).
pub use crate::index_lifecycle::guidance::file_read::append_dedup_hint_footer;

/// Format a "Hook Adoption (current session)" section from hook-time workflow counters.
pub(crate) fn format_hook_adoption(snap: &HookAdoptionSnapshot) -> String {
    if snap.is_empty() {
        return String::new();
    }

    let total = snap.total_attempts();
    let routed = snap.total_routed();
    let percent = if total == 0 {
        0
    } else {
        ((routed as f64 / total as f64) * 100.0).round() as usize
    };

    let mut lines = vec![
        "── Hook Adoption (current session) ──".to_string(),
        format!("Owned workflows routed: {routed}/{total} ({percent}%)"),
    ];

    let total_no_sidecar = snap.source_read.no_sidecar
        + snap.source_search.no_sidecar
        + snap.repo_start.no_sidecar
        + snap.prompt_context.no_sidecar
        + snap.post_edit_impact.no_sidecar;
    let total_sidecar_error = snap.source_read.sidecar_error
        + snap.source_search.sidecar_error
        + snap.repo_start.sidecar_error
        + snap.prompt_context.sidecar_error
        + snap.post_edit_impact.sidecar_error;
    let fail_open_total = snap.total_fail_open();
    if fail_open_total > 0 {
        lines.push(format!(
            "Fail-open outcomes: {fail_open_total} (no sidecar {total_no_sidecar}, sidecar errors {total_sidecar_error})"
        ));
    } else {
        lines.push("Fail-open outcomes: 0".to_string());
    }

    // Show daemon fallback total if any occurred.
    let total_daemon = snap.source_read.daemon_fallback
        + snap.source_search.daemon_fallback
        + snap.repo_start.daemon_fallback
        + snap.prompt_context.daemon_fallback
        + snap.post_edit_impact.daemon_fallback;
    if total_daemon > 0 {
        lines.push(format!("Daemon fallback routed: {total_daemon}"));
        lines.push(
            "Daemon fallback counts as routed work: the hook reached the daemon even though the sidecar was unavailable."
                .to_string(),
        );
    }

    let mut push_workflow_line =
        |label: &str, counts: &crate::cli::hook::WorkflowAdoptionCounts| {
            if counts.total() == 0 {
                return;
            }
            let mut parts = vec![format!("routed {}", counts.routed)];
            if counts.daemon_fallback > 0 {
                parts.push(format!("daemon fallback {}", counts.daemon_fallback));
            }
            if counts.fail_open() > 0 && counts.no_sidecar > 0 {
                parts.push(format!("no sidecar {}", counts.no_sidecar));
            }
            if counts.fail_open() > 0 && counts.sidecar_error > 0 {
                parts.push(format!("sidecar errors {}", counts.sidecar_error));
            }
            lines.push(format!("{label}: {}", parts.join(", ")));
        };

    push_workflow_line("Source read", &snap.source_read);
    push_workflow_line("Source search", &snap.source_search);
    push_workflow_line("Repo start", &snap.repo_start);
    push_workflow_line("Prompt context", &snap.prompt_context);
    push_workflow_line("Post-edit impact", &snap.post_edit_impact);

    if let Some(first) = snap.first_repo_start {
        lines.push(format!("First repo start: {}", first.label()));
    }

    // Show a hint when all fail-open outcomes are due to no-sidecar.
    if snap.total_fail_open() > 0
        && snap.total_routed() == 0
        && total_daemon == 0
        && total_sidecar_error == 0
    {
        lines.push(String::new());
        lines.push("⚠ All hook attempts failed open (no sidecar found).".to_string());
        lines.push("  Start SymForge as an MCP server or run 'symforge daemon start'.".to_string());
    } else if fail_open_total > 0 && total_sidecar_error == 0 {
        lines.push(String::new());
        lines.push(
            "Fail-open here is mostly benign: hooks fired before a sidecar was reachable or on workflows intentionally left pass-through."
                .to_string(),
        );
    } else if total_sidecar_error > 0 {
        lines.push(String::new());
        lines.push(
            "Actionable note: sidecar errors are real routing failures and worth investigating separately from no-sidecar outcomes."
                .to_string(),
        );
    }

    lines.join("\n")
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

pub use crate::index_lifecycle::guidance::search_render::{
    ExploreResultViewInput, explore_result_view,
};

/// Format git temporal data for a single file: churn, ownership, co-changes, last commit.
pub fn co_changes_result_view(
    path: &str,
    history: &crate::live_index::git_temporal::GitFileHistory,
    limit: usize,
) -> String {
    let mut lines = Vec::new();

    lines.push(format!("Git temporal data for {path}"));
    lines.push(String::new());

    // Churn
    lines.push(format!(
        "Churn score: {:.2} ({} commits)",
        history.churn_score, history.commit_count
    ));

    // Last commit
    let c = &history.last_commit;
    lines.push(format!(
        "Last commit: {} {} — {} ({})",
        c.hash, c.timestamp, c.message_head, c.author
    ));
    lines.push(String::new());

    // Ownership
    if !history.contributors.is_empty() {
        lines.push("Ownership:".to_string());
        for contrib in &history.contributors {
            lines.push(format!(
                "  {}: {} commits ({:.0}%)",
                contrib.author, contrib.commit_count, contrib.percentage
            ));
        }
        lines.push(String::new());
    }

    // Co-changes
    if history.co_changes.is_empty() {
        lines.push(
            "No high-confidence co-changing files detected (needs at least 2 shared commits and Jaccard >= 0.15)."
                .to_string(),
        );
        if !history.weak_co_changes.is_empty() {
            lines.push(String::new());
            lines.push(format!(
                "Low-confidence candidates (top {}):",
                limit.min(history.weak_co_changes.len())
            ));
            for entry in history.weak_co_changes.iter().take(limit) {
                lines.push(format!(
                    "  {:<50} coupling: {:.3}  ({} shared commits)",
                    entry.path, entry.coupling_score, entry.shared_commits
                ));
            }
            lines.push(
                "These missed the strong co-change threshold and are advisory only.".to_string(),
            );
        }
    } else {
        lines.push(format!(
            "Co-changing files (top {}):",
            limit.min(history.co_changes.len())
        ));
        for entry in history.co_changes.iter().take(limit) {
            lines.push(format!(
                "  {:<50} coupling: {:.3}  ({} shared commits)",
                entry.path, entry.coupling_score, entry.shared_commits
            ));
        }
    }

    lines.join("\n")
}

/// Compute the impact summary for a just-edited path: the distinct dependent
/// **file** count and the top-K co-change partner paths.
///
/// - Dependents use `capture_find_dependents_view(path).files.len()` — the count
///   of distinct importing/referencing files (matching the `find_dependents`
///   tool), NOT the raw per-reference count which double-counts a file that holds
///   multiple references.
/// - Co-changes are present only when `temporal.state` is `Ready` and the edited
///   path has a non-empty strong `co_changes` list; otherwise the returned vector
///   is empty (degrading the footer to `[impact: N dependents]`). The path is
///   forward-slash normalized before the temporal lookup to match the temporal
///   index key space.
///
/// `temporal` is passed in (rather than read off `index`) because the git
/// temporal snapshot lives on the shared index handle, not the `LiveIndex` read
/// snapshot. The caller (`append_impact_footer`) holds the handle and supplies
/// both sources.
pub fn edit_impact_summary(
    index: &LiveIndex,
    temporal: &crate::live_index::git_temporal::GitTemporalIndex,
    path: &str,
) -> (usize, Vec<String>) {
    const COCHANGE_LIMIT: usize = 3;

    let deps = index.capture_find_dependents_view(path).files.len();

    let normalized = path.replace('\\', "/");
    let cochanges = if temporal.state == crate::live_index::git_temporal::GitTemporalState::Ready {
        temporal
            .files
            .get(&normalized)
            .filter(|history| !history.co_changes.is_empty())
            .map(|history| {
                history
                    .co_changes
                    .iter()
                    .take(COCHANGE_LIMIT)
                    .map(|entry| entry.path.clone())
                    .collect::<Vec<String>>()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    (deps, cochanges)
}

/// Substrings that `classify_edit_output` (src/protocol/edit_tools.rs) treats as
/// failure / dry-run sentinels via `.contains(...)`. The three `_tool` wrappers
/// re-classify the FULL edit body — footer included — so a co-change partner
/// path that embeds one of these (e.g. `src/unavailable.rs`) would flip a
/// successful edit to a failure. Any partner whose path contains one of these is
/// elided from the footer. Keep in sync with `classify_edit_output`'s `.contains`
/// checks (only substring checks matter here; `starts_with` sentinels cannot be
/// triggered by a suffix footer).
const FOOTER_SENTINEL_SUBSTRINGS: &[&str] = &[
    "unavailable",
    "still loading",
    "no repository root configured",
    "Write failed",
    "ROLLBACK INCOMPLETE",
    "File disappeared:",
    "byte range",
    "Session stale",
    "Ambiguous:",
    "Symbol not found:",
    "File not indexed:",
    "path escapes repo root",
    "Path containment error",
    "Path resolution error",
    "[DRY RUN]",
    "Write semantics: dry run (no writes)",
];

/// Render the success-only post-edit impact footer.
///
/// `[impact: N dependents]` when `cochanges` is empty, otherwise
/// `[impact: N dependents · cochanges: a, b, c]` (partners joined with `, `).
/// The static text avoids every `classify_edit_output` sentinel substring, and
/// any co-change partner path that would itself embed a sentinel is filtered out
/// (see `FOOTER_SENTINEL_SUBSTRINGS`) so appending the footer to a successful
/// edit body can never flip the outcome class.
pub fn impact_footer(deps: usize, cochanges: &[String]) -> String {
    let safe: Vec<&str> = cochanges
        .iter()
        .map(String::as_str)
        .filter(|partner| {
            !FOOTER_SENTINEL_SUBSTRINGS
                .iter()
                .any(|sentinel| partner.contains(sentinel))
        })
        .collect();
    if safe.is_empty() {
        format!("[impact: {deps} dependents]")
    } else {
        format!(
            "[impact: {deps} dependents \u{00b7} cochanges: {}]",
            safe.join(", ")
        )
    }
}

/// Format symbol-level diff between two git refs.
///
/// `live` is required because EVERY content read in this function is a
/// disclosure lane, not just the working-tree one. An earlier version of this
/// comment scoped the hazard to "uncommitted mode reads the WORKING TREE",
/// gated that single read, and left the two `file_at_ref` reads beside it
/// ungated — so a security-demoted file's symbol names and signatures rendered
/// straight into any committed-vs-committed diff. Both git-object reads now go
/// through the same admission gate.
///
/// A refused file is reported as withheld rather than rendered as empty —
/// rendering it empty would state, falsely, that every one of its symbols was
/// removed.
pub fn diff_symbols_result_view(
    base: &str,
    target: &str,
    changed_files: &[&str],
    repo: &crate::git::GitRepo,
    live: &crate::live_index::LiveIndex,
    compact: bool,
    summary_only: bool,
) -> String {
    let view = crate::index_lifecycle::guidance::changes::capture_symbol_diff(
        base,
        target,
        changed_files,
        |reference, path| {
            if reference.is_empty() {
                crate::protocol::read_gate::admit_worktree_text(live, repo, path)
            } else {
                crate::protocol::read_gate::admit_git_text(live, repo, reference, path)
            }
        },
    );
    crate::index_lifecycle::guidance::changes::render_symbol_diff(&view, compact, summary_only)
}

/// Try to extract a declaration name from a line of code.
#[cfg(test)]
pub(crate) use crate::index_lifecycle::guidance::symbol_context::extract_declaration_name;

#[cfg(test)]
mod sfb15_tests {
    use super::*;

    #[test]
    fn impact_footer_elides_partner_paths_carrying_classifier_sentinels() {
        // A co-change partner whose path embeds a `classify_edit_output`
        // `.contains` sentinel (here `unavailable`) must NOT leak into the
        // footer, or the three `_tool` wrappers would re-classify a successful
        // destructive edit as a failure.
        let footer = impact_footer(
            2,
            &[
                "src/unavailable.rs".to_string(),
                "src/protocol/format.rs".to_string(),
            ],
        );
        assert!(
            !footer.contains("unavailable"),
            "footer must not carry a sentinel-bearing partner path: {footer}"
        );
        assert!(
            footer.contains("src/protocol/format.rs"),
            "safe partner should remain: {footer}"
        );
        assert!(footer.starts_with("[impact: 2 dependents"));

        // When every partner is sentinel-bearing, the cochanges clause is
        // dropped entirely, degrading to the dependents-only form.
        let all_filtered = impact_footer(3, &["build/unavailable_state.rs".to_string()]);
        assert_eq!(all_filtered, "[impact: 3 dependents]");
    }

    #[test]
    fn explore_result_view_filters_weak_trivial_symbols_and_doc_only_patterns() {
        let symbol_hits = vec![
            (
                "new".to_string(),
                "fn".to_string(),
                "src/cache/builder.rs".to_string(),
            ),
            (
                "TokenCache".to_string(),
                "struct".to_string(),
                "src/cache/token_cache.rs".to_string(),
            ),
        ];
        let text_hits = vec![
            (
                "src/cache/token_cache.rs".to_string(),
                "/// Token cache mentions authentication.".to_string(),
                1,
            ),
            (
                "src/cache/token_cache.rs".to_string(),
                "let cache = TokenCache::new();".to_string(),
                8,
            ),
        ];
        let related_files = vec![("src/cache/token_cache.rs".to_string(), 2)];
        let symbol_scores = vec![0.20, 0.95];

        let output = explore_result_view(ExploreResultViewInput {
            label: "cache",
            symbol_hits: &symbol_hits,
            text_hits: &text_hits,
            related_files: &related_files,
            enriched_symbols: &[],
            symbol_impls: &[],
            symbol_deps: &[],
            derived_seed_terms: &[],
            derived_symbols: &[],
            derived_seed_files: &[],
            enriched_imports: &[],
            symbol_scores: &symbol_scores,
            depth: 1,
        });

        assert!(
            !output.contains("fn new"),
            "weak trivial symbol should be filtered: {output}"
        );
        assert!(
            output.contains("struct TokenCache"),
            "high-signal project symbol should remain visible: {output}"
        );
        assert!(
            output.contains("reason: strong match"),
            "visible suggestions should carry concise reason text: {output}"
        );
        assert!(
            !output.contains("/// Token cache"),
            "doc-only pattern hit should be filtered from explore guidance: {output}"
        );
        assert!(
            output.contains("let cache = TokenCache::new();"),
            "real code pattern should remain visible: {output}"
        );
    }

    #[test]
    fn explore_result_view_keeps_trivial_symbol_when_strongly_contextualized() {
        let symbol_hits = vec![(
            "new".to_string(),
            "fn".to_string(),
            "src/cache/builder.rs".to_string(),
        )];
        let symbol_scores = vec![0.92];

        let output = explore_result_view(ExploreResultViewInput {
            label: "builder",
            symbol_hits: &symbol_hits,
            text_hits: &[],
            related_files: &[],
            enriched_symbols: &[],
            symbol_impls: &[],
            symbol_deps: &[],
            derived_seed_terms: &[],
            derived_symbols: &[],
            derived_seed_files: &[],
            enriched_imports: &[],
            symbol_scores: &symbol_scores,
            depth: 1,
        });

        assert!(
            output.contains("fn new"),
            "strongly contextualized trivial symbol should remain visible: {output}"
        );
        assert!(
            output.contains("reason: strong match"),
            "kept trivial symbol should explain why it was suggested: {output}"
        );
    }
}

#[cfg(test)]
mod tests;
