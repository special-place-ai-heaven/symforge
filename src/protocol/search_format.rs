pub(crate) fn format_search_envelope(
    match_type: &str,
    source_authority: &str,
    parse_state: &str,
    completeness: &str,
    scope: &str,
    evidence: &str,
) -> String {
    // "Silence is the happy path": on a fully-trusted result collapse the four
    // invariant status lines (match type / source authority / parse state /
    // completeness) into one compact `Trust:` line, keeping Scope and Evidence
    // (the differential fields). Any deviation — non-index authority, partial or
    // degraded parse, or non-full completeness — keeps the full six-line envelope
    // so degraded/stale/truncated results stay loud.
    if source_authority == "current index"
        && parse_state == "parsed"
        && completeness.starts_with("full")
    {
        format!(
            "Trust: {match_type} | {source_authority} | {parse_state} | {completeness}\nScope: {scope}\nEvidence: {evidence}"
        )
    } else {
        format!(
            "Match type: {match_type}\nSource authority: {source_authority}\nParse state: {parse_state}\nCompleteness: {completeness}\nScope: {scope}\nEvidence: {evidence}"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::format_search_envelope;

    #[test]
    fn test_format_search_envelope() {
        // Trusted baseline collapses the four invariant status lines into one
        // compact `Trust:` line, preserving Scope and Evidence.
        let rendered = format_search_envelope(
            "constrained (literal)",
            "current index",
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
            "disk (refreshed)",
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
}
