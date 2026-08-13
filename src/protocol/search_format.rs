//! The result-envelope formatter and the measured authority axis it rides on.

/// The source-authority axis of a result envelope.
///
/// The envelope used to collapse on `source_authority == "current index"` — a
/// STRING equality, so any caller could assert currency without measuring it,
/// and one lane did: the context bundle passed the literal whenever it had not
/// disk-refreshed, collapsing the envelope even while the index was Degraded.
/// That was the exact defect the repo's reporting invariant names — the thing
/// that reports is not the thing that knows.
///
/// The collapse decision now rides on this type, and the type is honest by
/// construction: [`SourceAuthority::from_freshness`] is the ONLY constructor
/// that can produce a collapsible value, and it takes the index's own measured
/// `FreshnessStatus`. A lying literal is unrepresentable — there is no
/// constructor that accepts a caller-chosen string and marks it collapsible.
#[derive(Clone, Copy)]
pub(crate) struct SourceAuthority {
    label: &'static str,
    collapsible: bool,
}

impl SourceAuthority {
    /// The measured axis. Collapse is permitted exactly when the index itself
    /// reports `Current`; `Verifying` and `Degraded` stay loud and say so.
    pub(crate) fn from_freshness(freshness: &crate::domain::FreshnessStatus) -> Self {
        match freshness {
            crate::domain::FreshnessStatus::Current => Self {
                label: "current index",
                collapsible: true,
            },
            crate::domain::FreshnessStatus::Verifying => Self {
                label: "index (verifying against disk)",
                collapsible: false,
            },
            crate::domain::FreshnessStatus::Degraded { .. } => Self {
                label: "index (UNVERIFIED against disk)",
                collapsible: false,
            },
        }
    }

    /// An authority that never collapses the envelope: disk-refreshed reads,
    /// composite stores, git-object diffs. The label is display only and
    /// cannot buy the compact banner, whatever it says.
    pub(crate) fn never_collapse(label: &'static str) -> Self {
        Self {
            label,
            collapsible: false,
        }
    }

    pub(crate) fn label(&self) -> &'static str {
        self.label
    }
}

pub(crate) fn format_search_envelope(
    match_type: &str,
    source_authority: SourceAuthority,
    parse_state: &str,
    completeness: &str,
    scope: &str,
    evidence: &str,
) -> String {
    // "Silence is the happy path": on a fully-trusted result collapse the four
    // invariant status lines (match type / source authority / parse state /
    // completeness) into one compact `Trust:` line, keeping Scope and Evidence
    // (the differential fields). Any deviation — a non-collapsible authority,
    // partial or degraded parse, or non-full completeness — keeps the full
    // six-line envelope so degraded/stale/truncated results stay loud.
    let label = source_authority.label();
    if source_authority.collapsible && parse_state == "parsed" && completeness.starts_with("full") {
        format!(
            "Trust: {match_type} | {label} | {parse_state} | {completeness}\nScope: {scope}\nEvidence: {evidence}"
        )
    } else {
        format!(
            "Match type: {match_type}\nSource authority: {label}\nParse state: {parse_state}\nCompleteness: {completeness}\nScope: {scope}\nEvidence: {evidence}"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{SourceAuthority, format_search_envelope};

    #[test]
    fn test_format_search_envelope() {
        // Trusted baseline collapses the four invariant status lines into one
        // compact `Trust:` line, preserving Scope and Evidence.
        let rendered = format_search_envelope(
            "constrained (literal)",
            SourceAuthority::from_freshness(&crate::domain::FreshnessStatus::Current),
            "parsed",
            "full for current scope",
            "repo-wide; tests filtered; generated filtered",
            "line anchors `src/lib.rs:7`, `src/mod.rs:12`",
        );

        assert!(rendered.contains("Trust: constrained (literal) | current index | parsed | full"));
        assert!(!rendered.contains("Source authority:"));
        assert!(!rendered.contains("Parse state:"));
        assert!(!rendered.contains("Completeness:"));
        assert!(rendered.contains("Scope: repo-wide; tests filtered; generated filtered"));
        assert!(rendered.contains("Evidence: line anchors `src/lib.rs:7`, `src/mod.rs:12`"));
    }

    #[test]
    fn test_format_search_envelope_keeps_full_envelope_on_deviation() {
        // Any deviation from the trusted baseline keeps the full six-line envelope
        // so degraded / stale / truncated results stay loud.
        let rendered = format_search_envelope(
            "exact",
            SourceAuthority::never_collapse("disk (refreshed)"),
            "partial",
            "truncated by result cap (3 more omitted)",
            "path `src/lib.rs`",
            "line anchors `src/lib.rs:7`",
        );

        assert!(rendered.contains("Match type: exact"));
        assert!(rendered.contains("Source authority: disk (refreshed)"));
        assert!(rendered.contains("Parse state: partial"));
        assert!(rendered.contains("Completeness: truncated by result cap (3 more omitted)"));
        assert!(rendered.contains("Scope: path `src/lib.rs`"));
        assert!(rendered.contains("Evidence: line anchors `src/lib.rs:7`"));
    }

    #[test]
    fn a_measured_degraded_authority_never_collapses_however_clean_the_rest_is() {
        // The axis that used to be forgeable: parse and completeness are both
        // pristine, and the ONLY deviation is the measured freshness. The
        // envelope must stay loud — this is the case the string comparison let
        // a lane collapse by asserting the literal.
        for freshness in [
            crate::domain::FreshnessStatus::Verifying,
            crate::domain::FreshnessStatus::Degraded {
                last_valid_content_generation: 1,
                reason_codes: vec![crate::domain::FreshnessReason::ObservationFailed],
            },
        ] {
            let rendered = format_search_envelope(
                "exact",
                SourceAuthority::from_freshness(&freshness),
                "parsed",
                "full for current scope",
                "repo-wide",
                "line anchors `src/lib.rs:7`",
            );
            assert!(
                rendered.contains("Match type: exact"),
                "a non-Current measurement keeps the loud envelope: {rendered}"
            );
            assert!(
                !rendered.starts_with("Trust:"),
                "a non-Current measurement must not buy the compact banner: {rendered}"
            );
        }
    }
}
