//! Shared admission refusal wording.

pub fn content_withheld_by_admission<S: AsRef<str>>(
    path: &str,
    rule_ids: &[S],
    finding_count: u32,
    line_ranges: &[(u32, u32)],
) -> String {
    let rules = rule_ids
        .iter()
        .map(AsRef::as_ref)
        .collect::<Vec<_>>()
        .join(", ");
    let at = match line_ranges {
        [] => String::new(),
        [(start, end)] if start == end => format!(" at line {start}"),
        _ => format!(" at lines {}", render_line_ranges(line_ranges)),
    };
    format!(
        "Content withheld by admission policy: {path}. \
         Secret detector rule{} {rules} matched {finding_count} time{}{at}; \
         SymForge will not disclose, parse, or search this file.",
        if rule_ids.len() == 1 { "" } else { "s" },
        if finding_count == 1 { "" } else { "s" },
    )
}

pub fn content_withheld_by_path_rule(path: &str, rule_id: &str) -> String {
    format!(
        "Content withheld by admission policy: {path}. \
         Path rule {rule_id} excludes this file by name as a credential file; \
         SymForge will not disclose, parse, or search it."
    )
}

fn render_line_ranges(ranges: &[(u32, u32)]) -> String {
    const SHOWN: usize = 10;
    let mut rendered = ranges
        .iter()
        .take(SHOWN)
        .map(|&(start, end)| {
            if start == end {
                start.to_string()
            } else {
                format!("{start}-{end}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    if ranges.len() > SHOWN {
        rendered.push_str(" and more");
    }
    rendered
}

pub fn content_withheld_unscanned(path: &str) -> String {
    format!(
        "Content withheld by admission policy: {path}. \
         SymForge could not inspect this file's contents and will not read, \
         parse, or search it. Reindexing will not change this."
    )
}

pub fn unverified_since_restore(path: &str, reason: &str) -> String {
    format!(
        "Unverified since restore: {path}. The snapshot verify could not reconcile this \
         file ({reason}), so its restored content is withheld instead of served as current. \
         A successful re-read releases it: the watcher does that on the next change to the \
         file, and index_folder rebuilds the whole project from source."
    )
}
