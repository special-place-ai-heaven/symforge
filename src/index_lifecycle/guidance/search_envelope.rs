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
    /// The measured freshness is degraded (SF-DOG-04): the Completeness line
    /// must not read as a plain "full".
    stale: bool,
}

/// The scope [`search_scope_summary`] renders for a search with every filter
/// at its default. A collapsed envelope omits a scope equal to this.
pub(crate) const DEFAULT_SEARCH_SCOPE: &str = "repo-wide; tests filter active (heuristic); generated filter active (heuristic); vendor filter active (heuristic); personal tooling filtered";

/// The scope `search_files` renders for its default path ranking with default
/// filters. Other ranking modes never collapse the envelope.
pub(crate) const DEFAULT_SEARCH_FILES_SCOPE: &str = "ranked indexed file paths; filters: vendor filter active (heuristic); personal tooling filtered";

/// Completeness fragment present exactly when a noise filter hid matches.
const NOISE_SUPPRESSED_FRAGMENT: &str = "noise-filtered match(es) suppressed";

impl SourceAuthority {
    /// The measured axis. Collapse is permitted exactly when the index itself
    /// reports `Current`; `Verifying` and `Degraded` stay loud and say so.
    pub(crate) fn from_freshness(freshness: &crate::domain::FreshnessStatus) -> Self {
        use crate::live_index::store::TrustHeadline;
        // Readiness is not visible at this seam, so this passes Ready and keeps
        // only the freshness-derived qualifier of the headline.
        let stale = matches!(
            TrustHeadline::derive(&crate::live_index::PublishedIndexStatus::Ready, freshness),
            TrustHeadline::ReadyStale | TrustHeadline::Stale
        );
        match freshness {
            crate::domain::FreshnessStatus::Current => Self {
                label: "current index",
                collapsible: true,
                stale,
            },
            crate::domain::FreshnessStatus::Verifying => Self {
                label: "index (verifying against disk)",
                collapsible: false,
                stale,
            },
            crate::domain::FreshnessStatus::Degraded { .. } => Self {
                label: "index (UNVERIFIED against disk)",
                collapsible: false,
                stale,
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
            stale: false,
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
    // completeness) into one compact `Trust:` line. A scope at the default
    // search scope is omitted, and with it the evidence anchors, which repeat
    // the hits listed below (SF-TE-06): a default search gets one line. Any
    // other scope keeps its Scope and Evidence lines. Any deviation -- a
    // non-collapsible authority, partial or degraded parse, or non-full
    // completeness -- keeps the full six-line envelope so degraded/stale/
    // truncated results stay loud.
    let label = source_authority.label();
    if source_authority.collapsible && parse_state == "parsed" && completeness.starts_with("full") {
        let trust = format!("Trust: {match_type} | {label} | {parse_state} | {completeness}");
        // A filter that hid matches is a non-default condition: the scope
        // line names which filters were active.
        let default_scope = (scope == DEFAULT_SEARCH_SCOPE || scope == DEFAULT_SEARCH_FILES_SCOPE)
            && !completeness.contains(NOISE_SUPPRESSED_FRAGMENT);
        if default_scope {
            trust
        } else {
            format!("{trust}\nScope: {scope}\nEvidence: {evidence}")
        }
    } else {
        // SF-DOG-04: a "full" answer from a stale index says so.
        let stale = if source_authority.stale {
            "; index may be stale (freshness degraded; health names the cause)"
        } else {
            ""
        };
        format!(
            "Match type: {match_type}\nSource authority: {label}\nParse state: {parse_state}\nCompleteness: {completeness}{stale}\nScope: {scope}\nEvidence: {evidence}"
        )
    }
}

// ── Search envelope pieces shared by the MCP search tools and the embedded
// facade (moved verbatim from `protocol::tools`). ──

use super::reference_read::anchored_search_evidence;
use crate::domain::LanguageId;
use crate::live_index::search;
use crate::live_index::{SearchFilesTier, SearchFilesView};

pub(crate) fn search_scope_summary(
    path_scope: &search::PathScope,
    language_filter: Option<&LanguageId>,
    noise_policy: &search::NoisePolicy,
    include_personal_tooling: bool,
    glob: Option<&str>,
    exclude_glob: Option<&str>,
    ranked: bool,
) -> String {
    let mut parts = Vec::new();
    match path_scope {
        search::PathScope::Any => parts.push("repo-wide".to_string()),
        search::PathScope::Exact(path) => parts.push(format!("path `{path}`")),
        search::PathScope::Prefix(prefix) => parts.push(format!("path prefix `{prefix}`")),
    }
    if let Some(language) = language_filter {
        parts.push(format!("language `{language}`"));
    }
    // SF-STRESS-011 honesty fix: these are HEURISTIC path-based filters, not a
    // guaranteed outcome. Detection keys on path segments (vendor/, deps/,
    // dist/, test_data/, ...) and basename patterns, so a vendored/generated
    // file with an unconventional path can still appear in results. The header
    // says "filter active (heuristic)" rather than asserting the file class was
    // actually removed, which the previous "vendor filtered" wording overstated.
    parts.push(if noise_policy.include_tests {
        "tests included".to_string()
    } else {
        "tests filter active (heuristic)".to_string()
    });
    parts.push(if noise_policy.include_generated {
        "generated included".to_string()
    } else {
        "generated filter active (heuristic)".to_string()
    });
    parts.push(if noise_policy.include_vendor {
        "vendor included".to_string()
    } else {
        "vendor filter active (heuristic)".to_string()
    });
    parts.push(if include_personal_tooling {
        "personal tooling included".to_string()
    } else {
        "personal tooling filtered".to_string()
    });
    if let Some(glob) = glob {
        parts.push(format!("glob `{glob}`"));
    }
    if let Some(exclude_glob) = exclude_glob {
        parts.push(format!("exclude `{exclude_glob}`"));
    }
    if ranked {
        parts.push("ranked ordering enabled".to_string());
    }
    parts.join("; ")
}

pub(crate) fn search_completeness_label(
    overflow_count: usize,
    suppressed_by_noise: usize,
) -> String {
    // Honesty (trust): the index is built by a discovery walk with the `ignore`
    // crate default `.hidden(true)`, so hidden / dotdir paths (`.github/`,
    // `.gitlab-ci.yml`, …) are NOT indexed and never appear in results. A bare
    // "full" claim would mislead an agent into trusting the file count as
    // exhaustive (the dogfood report: `search_text` silently omitted
    // `.github/workflows/release-please.yml` that ripgrep found). Qualify the
    // claim so the agent knows to use a raw grep for hidden paths.
    let mut parts = vec![if overflow_count > 0 {
        format!("truncated by result cap ({overflow_count} more omitted)")
    } else {
        "full for indexed scope (hidden/dotdir paths not indexed — grep those)".to_string()
    }];
    if suppressed_by_noise > 0 {
        if suppressed_by_noise > search::SUPPRESSED_TEXT_MATCH_DISPLAY_CAP {
            parts.push(format!(
                "{}+ noise-filtered match(es) suppressed",
                search::SUPPRESSED_TEXT_MATCH_DISPLAY_CAP
            ));
        } else {
            parts.push(format!(
                "{suppressed_by_noise} noise-filtered match(es) suppressed"
            ));
        }
    }
    parts.join("; ")
}

pub(crate) fn search_text_match_type_label(
    structural: bool,
    is_regex: bool,
    terms: Option<&[String]>,
    auto_detected_regex: bool,
    auto_corrected_regex: bool,
    ranked: bool,
) -> String {
    if structural {
        "structural (ast-grep)".to_string()
    } else if auto_corrected_regex {
        "heuristic (auto-corrected regex)".to_string()
    } else if is_regex && auto_detected_regex {
        "heuristic (auto-detected regex)".to_string()
    } else if is_regex {
        "heuristic (regex)".to_string()
    } else if ranked {
        match terms {
            Some(terms) if !terms.is_empty() => "heuristic (ranked OR-literal terms)".to_string(),
            _ => "heuristic (ranked literal)".to_string(),
        }
    } else if matches!(terms, Some(terms) if !terms.is_empty()) {
        "constrained (OR-literal terms)".to_string()
    } else {
        "constrained (literal)".to_string()
    }
}

pub(crate) fn search_symbols_match_type_label(
    result: &search::SymbolSearchResult,
    is_browse: bool,
) -> &'static str {
    if is_browse {
        "constrained (scoped browse)"
    } else {
        match result.hits.first().map(|hit| hit.tier) {
            Some(search::SymbolMatchTier::Exact) => "exact",
            Some(search::SymbolMatchTier::Prefix) => "constrained (prefix tier)",
            Some(search::SymbolMatchTier::Substring) => "heuristic (substring tier)",
            None => "constrained",
        }
    }
}

pub(crate) fn search_files_match_type_label(view: &SearchFilesView) -> &'static str {
    match view {
        SearchFilesView::Found { hits, .. } => match hits.first().map(|hit| hit.tier) {
            Some(SearchFilesTier::CoChange) => "heuristic (git-temporal coupling)",
            Some(SearchFilesTier::StrongPath) | Some(SearchFilesTier::Basename) => {
                "constrained (tiered path relevance)"
            }
            Some(SearchFilesTier::LoosePath) => "heuristic (loose path relevance)",
            Some(SearchFilesTier::MetadataOnly) => "heuristic (Tier-2 metadata-only path)",
            None => "constrained",
        },
        _ => "constrained",
    }
}

pub(crate) fn search_text_evidence(result: &search::TextSearchResult) -> String {
    let anchors = result
        .files
        .iter()
        .flat_map(|file| {
            file.matches
                .iter()
                .take(2)
                .map(move |line_match| format!("{}:{}", file.path, line_match.line_number))
        })
        .take(3)
        .collect();
    anchored_search_evidence(anchors, "line anchors")
}

pub(crate) fn search_symbols_evidence(result: &search::SymbolSearchResult) -> String {
    let anchors = result
        .hits
        .iter()
        .take(3)
        .map(|hit| format!("{}:{}", hit.path, hit.line))
        .collect();
    anchored_search_evidence(anchors, "symbol anchors")
}

pub(crate) fn search_files_filter_summary(
    include_vendor: bool,
    include_personal_tooling: bool,
) -> String {
    // SF-STRESS-011 honesty fix: vendor detection is a heuristic path filter, so
    // the header says "filter active (heuristic)" rather than asserting the file
    // class was actually removed. Personal-tooling paths are an exact prefix
    // match, so that claim stays definite.
    let vendor = if include_vendor {
        "vendor included"
    } else {
        "vendor filter active (heuristic)"
    };
    let personal = if include_personal_tooling {
        "personal tooling included"
    } else {
        "personal tooling filtered"
    };
    format!("filters: {vendor}; {personal}")
}

pub(crate) fn search_files_scope_summary(
    base_scope: impl Into<String>,
    include_vendor: bool,
    include_personal_tooling: bool,
) -> String {
    format!(
        "{}; {}",
        base_scope.into(),
        search_files_filter_summary(include_vendor, include_personal_tooling)
    )
}

pub(crate) fn append_search_files_filter_summary(
    result: &mut String,
    include_vendor: bool,
    include_personal_tooling: bool,
) {
    result.push_str("\n\n");
    result.push_str(&search_files_filter_summary(
        include_vendor,
        include_personal_tooling,
    ));
}

pub(crate) fn search_files_hidden_noise_note(
    hidden_count: usize,
    include_vendor: bool,
    include_personal_tooling: bool,
) -> Option<String> {
    if hidden_count == 0 || (include_vendor && include_personal_tooling) {
        return None;
    }

    let mut flags = Vec::new();
    if !include_vendor {
        flags.push("include_vendor=true");
    }
    if !include_personal_tooling {
        flags.push("include_personal_tooling=true");
    }

    let noun = if hidden_count == 1 {
        "path candidate"
    } else {
        "path candidates"
    };
    Some(format!(
        "{hidden_count} vendor/personal-tooling {noun} hidden by default; pass {} to include suppressed noise.",
        flags.join(" or ")
    ))
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
    fn default_search_envelope_is_one_line_and_non_default_fields_still_print() {
        let noise = crate::live_index::search::NoisePolicy {
            include_generated: false,
            include_tests: false,
            include_vendor: false,
            include_ignored: false,
        };
        let default_scope = super::search_scope_summary(
            &crate::live_index::search::PathScope::Any,
            None,
            &noise,
            false,
            None,
            None,
            false,
        );
        assert_eq!(default_scope, super::DEFAULT_SEARCH_SCOPE);
        let completeness = super::search_completeness_label(0, 0);
        let current = || SourceAuthority::from_freshness(&crate::domain::FreshnessStatus::Current);

        let one = format_search_envelope(
            "constrained (literal)",
            current(),
            "parsed",
            &completeness,
            &default_scope,
            "line anchors `src/lib.rs:7`",
        );
        assert_eq!(one.lines().count(), 1, "{one}");
        assert!(one.starts_with("Trust: constrained (literal) | current index | parsed | full"));

        let scoped = format_search_envelope(
            "constrained (literal)",
            current(),
            "parsed",
            &completeness,
            "path prefix `src/`",
            "line anchors `src/lib.rs:7`",
        );
        assert!(scoped.contains("\nScope: path prefix `src/`"), "{scoped}");
        assert!(scoped.contains("\nEvidence: line anchors `src/lib.rs:7`"));

        let truncated = format_search_envelope(
            "constrained (literal)",
            current(),
            "parsed",
            &super::search_completeness_label(3, 0),
            &default_scope,
            "line anchors `src/lib.rs:7`",
        );
        assert!(truncated.contains("\nCompleteness: truncated by result cap (3 more omitted)"));

        // A filter that hid matches is named, even at the default scope.
        let suppressed = format_search_envelope(
            "exact",
            current(),
            "parsed",
            &super::search_completeness_label(0, 2),
            &default_scope,
            "symbol anchors `src/lib.rs:7`",
        );
        assert!(
            suppressed.contains("\nScope: repo-wide; tests filter active (heuristic)"),
            "{suppressed}"
        );

        // search_files: the default ranking is a default scope; a boost is not.
        let files_scope =
            super::search_files_scope_summary("ranked indexed file paths", false, false);
        assert_eq!(files_scope, super::DEFAULT_SEARCH_FILES_SCOPE);
        let files_one = format_search_envelope(
            "constrained (tiered path relevance)",
            current(),
            "parsed",
            &completeness,
            &files_scope,
            "paths `src/lib.rs`",
        );
        assert_eq!(files_one.lines().count(), 1, "{files_one}");
        let boosted = format_search_envelope(
            "constrained (tiered path relevance)",
            current(),
            "parsed",
            &completeness,
            &super::search_files_scope_summary(
                "ranked indexed file paths; current file boost `src/a.rs`",
                false,
                false,
            ),
            "paths `src/lib.rs`",
        );
        assert!(boosted.contains("\nScope: ranked indexed file paths; current file boost"));
    }

    #[test]
    fn stale_freshness_qualifies_a_full_completeness_line() {
        let stale = format_search_envelope(
            "exact",
            SourceAuthority::from_freshness(&crate::domain::FreshnessStatus::Degraded {
                last_valid_content_generation: 1,
                reason_codes: vec![crate::domain::FreshnessReason::ObservationFailed],
            }),
            "parsed",
            "full for indexed scope",
            super::DEFAULT_SEARCH_SCOPE,
            "line anchors `src/lib.rs:7`",
        );
        assert!(
            stale.contains("\nCompleteness: full for indexed scope; index may be stale"),
            "{stale}"
        );
        let verifying = format_search_envelope(
            "exact",
            SourceAuthority::from_freshness(&crate::domain::FreshnessStatus::Verifying),
            "parsed",
            "full for indexed scope",
            super::DEFAULT_SEARCH_SCOPE,
            "line anchors `src/lib.rs:7`",
        );
        assert!(!verifying.contains("index may be stale"), "{verifying}");
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
