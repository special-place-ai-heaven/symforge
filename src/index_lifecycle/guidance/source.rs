//! Pure source projections shared by all engine adapters.

pub fn is_noise_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.starts_with("///") || trimmed.starts_with("//!") || trimmed.starts_with("/**") {
        return false;
    }
    trimmed.starts_with("use ")
        || trimmed.starts_with("import ")
        || (trimmed.starts_with("from ") && trimmed.contains(" import "))
        || trimmed.starts_with("require(")
        || trimmed.starts_with("#include")
        || trimmed.starts_with("//")
        || (trimmed.starts_with("# ")
            && !trimmed.starts_with("# TODO")
            && !trimmed.starts_with("# FIXME")
            && !trimmed.starts_with("# NOTE")
            && !trimmed.starts_with("# HACK")
            && !trimmed.starts_with("# type:"))
        || trimmed == "#"
        || trimmed.starts_with("#!")
        || trimmed.starts_with("/*")
        || trimmed.starts_with("* ")
        || trimmed.starts_with("*/")
        || trimmed.starts_with("--")
        || (trimmed.starts_with("const ") && trimmed.contains("require("))
        || (trimmed.starts_with("let ") && trimmed.contains("require("))
        || (trimmed.starts_with("var ") && trimmed.contains("require("))
}

pub(crate) fn extract_signature(body: &str) -> String {
    let mut sig_lines: Vec<&str> = Vec::new();
    let mut in_sig = false;

    for line in body.lines() {
        let trimmed = line.trim();

        if !in_sig {
            // Skip leading empty lines and doc/attribute comments
            if trimmed.is_empty()
                || trimmed.starts_with("///")
                || trimmed.starts_with("//!")
                || trimmed.starts_with("//")
                || trimmed.starts_with("/**")
                || trimmed.starts_with("/*")
                || trimmed.starts_with('*')
                || trimmed.starts_with('#')
            {
                continue;
            }
            in_sig = true;
        }

        sig_lines.push(trimmed);

        // Stop once the signature is terminated:
        // - opens a body block: `{`
        // - `where` clause (generics constraints) — include the where line then stop at `{`
        // - declaration terminator: `;` (abstract methods, type aliases, extern fns)
        // - `=>` (match arm / single-expr lambda — stop collecting)
        if trimmed.ends_with('{')
            || trimmed.ends_with("where")
            || trimmed.ends_with(';')
            || trimmed == "{"
        {
            break;
        }
        // A line that IS just a where clause body — keep collecting until `{`
        // A plain `)` or `->` line means multi-line sig still continuing — keep going
        // But cap at 10 lines to avoid pulling in the entire body for edge cases
        if sig_lines.len() >= 10 {
            break;
        }
    }

    if sig_lines.is_empty() {
        return body.lines().next().unwrap_or("").to_string();
    }

    // Join multi-line signatures onto a single line, collapsing extra whitespace
    let joined = sig_lines.join(" ");
    // Strip trailing ` {` or ` ;` from the end — the signature line should not
    // include the opening brace or semicolon
    let result = joined
        .trim_end_matches(" {")
        .trim_end_matches('{')
        .trim_end_matches(';')
        .trim();
    result.to_string()
}

pub(crate) const CANONICAL_TRUNCATION_MARKER: &str = "[truncated]";
pub(crate) fn canonical_truncation_notice(amount: u64, unit: &str, detail: Option<&str>) -> String {
    match detail.filter(|value| !value.is_empty()) {
        Some(detail) => {
            format!("{CANONICAL_TRUNCATION_MARKER} Truncated at ~{amount} {unit}. {detail}")
        }
        None => format!("{CANONICAL_TRUNCATION_MARKER} Truncated at ~{amount} {unit}."),
    }
}

pub(crate) fn token_truncation_notice(max_tokens: u64, detail: Option<&str>) -> String {
    canonical_truncation_notice(max_tokens, "tokens", detail)
}

pub(crate) fn token_truncation_footer(max_tokens: u64, detail: Option<&str>) -> String {
    format!("\n{}\n", token_truncation_notice(max_tokens, detail))
}

/// Enforce a max-token budget on an already-assembled output string.
///
/// If the output exceeds `max_tokens * 4` bytes it is truncated at a line
/// boundary and a clear notice is appended.  Returns the original string
/// unchanged when no budget is set or the output fits within the budget.
pub fn enforce_token_budget(output: String, max_tokens: Option<u64>) -> String {
    enforce_token_budget_flagged(output, max_tokens).0
}

/// [`enforce_token_budget`] that also reports WHETHER it truncated, so callers
/// that already stamped a trust envelope can downgrade a now-stale
/// `Completeness: full` claim (dogfood 2026-07-11: `get_file_context` cut its
/// assembled output after the sidecar stamped `full`).
pub fn enforce_token_budget_flagged(output: String, max_tokens: Option<u64>) -> (String, bool) {
    let max_tokens = match max_tokens {
        Some(t) if t > 0 => t,
        _ => return (output, false),
    };
    let max_bytes = (max_tokens as usize).saturating_mul(4);
    if output.len() <= max_bytes {
        return (output, false);
    }
    let actual_tokens_est = crate::stel::handler::estimate_tokens_from_bytes(output.len() as u64);
    let mut truncated = truncate_text_at_line_boundary(&output, max_bytes);
    let detail =
        format!("Original output is ~{actual_tokens_est} tokens; budget is {max_tokens} tokens.");
    truncated.push_str(&token_truncation_footer(max_tokens, Some(detail.as_str())));
    (truncated, true)
}

/// Result of [`enforce_sectioned_budget`].
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SectionedOutput {
    pub text: String,
    /// Names of the optional sections that were left out, in the order they were dropped.
    pub dropped: Vec<String>,
    /// True when the primary content itself had to be cut.
    pub truncated: bool,
}

/// Fit `primary` plus `optional` sections into `limit_bytes` (feature 036, FR-024/025/026).
///
/// `optional` is in drop order: the first entry is dropped first, and the survivors keep
/// their order after the primary text. The primary text is cut only when it alone exceeds
/// the limit, at a line boundary. Whatever was left out is counted in a closing line that
/// ends with `continuation`, the literal call that returns the rest.
pub(crate) fn enforce_sectioned_budget(
    primary: &str,
    optional: &[(&str, &str)],
    limit_bytes: usize,
    continuation: &dyn Fn(&[String], bool) -> String,
) -> SectionedOutput {
    const SEPARATOR: &str = "\n\n";
    let total = |from: usize| {
        primary.len()
            + optional[from..]
                .iter()
                .map(|(_, body)| SEPARATOR.len() + body.len())
                .sum::<usize>()
    };
    if total(0) <= limit_bytes {
        let mut text = primary.to_string();
        for (_, body) in optional {
            text.push_str(SEPARATOR);
            text.push_str(body);
        }
        return SectionedOutput {
            text,
            dropped: Vec::new(),
            truncated: false,
        };
    }
    let names: Vec<String> = optional
        .iter()
        .map(|(name, _)| (*name).to_string())
        .collect();
    // The closing line is appended after fitting, so room for its longest form (every
    // section dropped, primary cut) is reserved first: the result stays within the limit.
    let reserve = cut_footer(&names, true, &continuation(&names, true)).len();
    let fit = limit_bytes.saturating_sub(reserve);
    let mut first_kept = 0;
    while first_kept < optional.len() && total(first_kept) > fit {
        first_kept += 1;
    }
    let dropped: Vec<String> = names[..first_kept].to_vec();
    let truncated = primary.len() > fit;
    let mut text = if truncated {
        truncate_text_at_line_boundary(primary, fit)
    } else {
        primary.to_string()
    };
    for (_, body) in &optional[first_kept..] {
        text.push_str(SEPARATOR);
        text.push_str(body);
    }
    text.push_str(&cut_footer(
        &dropped,
        truncated,
        &continuation(&dropped, truncated),
    ));
    // A cut answer never keeps a `Completeness: full` claim stamped before the cut.
    let text = downgrade_full_completeness_after_truncation(&text);
    SectionedOutput {
        text,
        dropped,
        truncated,
    }
}

/// The closing line of a shortened answer: what was left out and the literal next call.
fn cut_footer(dropped: &[String], truncated: bool, next: &str) -> String {
    let mut left_out = Vec::new();
    if truncated {
        left_out.push("the tail of the primary content".to_string());
    }
    if !dropped.is_empty() {
        left_out.push(format!(
            "{} optional section(s): {}",
            dropped.len(),
            dropped.join(", ")
        ));
    }
    format!(
        "\n[truncated] Output limit reached; left out {}. Next: {next}\n",
        left_out.join(" and ")
    )
}

/// The hard cap that applies when the caller gave no `max_tokens`: a default never cuts
/// an answer below `GET_FILE_CONTENT_MAX_BYTES`, and past it the answer says how much
/// was shown and ends with the literal call from `continuation`.
pub(crate) fn enforce_default_output_cap(
    output: String,
    continuation: impl FnOnce(usize) -> String,
) -> String {
    let cap = super::file_read::GET_FILE_CONTENT_MAX_BYTES;
    if output.len() <= cap {
        return output;
    }
    let next = continuation(output.len());
    enforce_sectioned_budget(&output, &[], cap, &|_, _| next.clone()).text
}

fn quoted(text: &str) -> String {
    format!("{text:?}")
}

/// `get_symbol(...)` that returns a whole symbol the default cap cut: every argument that
/// selected it, plus a `max_tokens` above its size (an explicit budget lifts the cap).
pub(crate) fn symbol_read_continuation(
    path: &str,
    name: &str,
    kind: Option<&str>,
    symbol_line: Option<u32>,
    bytes: usize,
) -> String {
    let mut call = format!("get_symbol(path={}, name={}", quoted(path), quoted(name));
    if let Some(kind) = kind {
        call.push_str(&format!(", kind={}", quoted(kind)));
    }
    if let Some(line) = symbol_line {
        call.push_str(&format!(", symbol_line={line}"));
    }
    call.push_str(&format!(", max_tokens={})", bytes / 4 + 256));
    call
}

/// The call(s) that return what a cut `get_file_context` left out: the dropped sections by
/// name, and the code sections again under an explicit budget when the primary was cut.
pub(crate) fn file_context_continuation(
    path: &str,
    sections: Option<&[String]>,
    include_tests: bool,
    dropped: &[String],
    truncated: bool,
    bytes: usize,
) -> String {
    let tests = if include_tests {
        ", include_tests=true"
    } else {
        ""
    };
    let mut calls = Vec::new();
    if !dropped.is_empty() {
        let names: Vec<String> = dropped.iter().map(|name| quoted(name)).collect();
        calls.push(format!(
            "get_file_context(path={}, sections=[{}]{tests})",
            quoted(path),
            names.join(", ")
        ));
    }
    if truncated {
        let sections = sections
            .filter(|sections| !sections.is_empty())
            .map(|sections| {
                let names: Vec<String> = sections.iter().map(|name| quoted(name)).collect();
                format!(", sections=[{}]", names.join(", "))
            })
            .unwrap_or_default();
        calls.push(format!(
            "get_file_context(path={}{sections}{tests}, max_tokens={})",
            quoted(path),
            bytes / 4 + 256
        ));
    }
    calls.join("; ")
}

/// As [`file_context_continuation`] for `get_symbol_context` trace mode; the selector
/// arguments (`symbol_kind`, `symbol_line`) travel with every call.
#[allow(clippy::too_many_arguments)] // every argument that selected the symbol travels
pub(crate) fn symbol_context_continuation(
    path: &str,
    name: &str,
    symbol_kind: Option<&str>,
    symbol_line: Option<u32>,
    sections: Option<&[String]>,
    include_tests: bool,
    dropped: &[String],
    truncated: bool,
    bytes: usize,
) -> String {
    let mut selector = format!("name={}, path={}", quoted(name), quoted(path));
    if let Some(kind) = symbol_kind {
        selector.push_str(&format!(", symbol_kind={}", quoted(kind)));
    }
    if let Some(line) = symbol_line {
        selector.push_str(&format!(", symbol_line={line}"));
    }
    if include_tests {
        selector.push_str(", include_tests=true");
    }
    let mut calls = Vec::new();
    if !dropped.is_empty() {
        let names: Vec<String> = dropped.iter().map(|name| quoted(name)).collect();
        calls.push(format!(
            "get_symbol_context({selector}, sections=[{}])",
            names.join(", ")
        ));
    }
    if truncated {
        // The caller's own sections, never a wider set: `[]` only if they asked for all.
        let original: Vec<String> = sections
            .unwrap_or_default()
            .iter()
            .map(|section| quoted(section))
            .collect();
        calls.push(format!(
            "get_symbol_context({selector}, sections=[{}], max_tokens={})",
            original.join(", "),
            bytes / 4 + 256
        ));
    }
    calls.join("; ")
}

/// Default (no `max_tokens`) hard cap for a `get_file_context` answer: `None` while
/// `total_len` fits, otherwise the primary code context with the optional sections that
/// still fit and a closing line naming the rest and the call that returns it.
pub(crate) fn cap_default_file_context(
    path: &str,
    sections: Option<&[String]>,
    include_tests: bool,
    primary: &str,
    optional: &[(&str, &str)],
    total_len: usize,
) -> Option<String> {
    let cap = super::file_read::GET_FILE_CONTENT_MAX_BYTES;
    if total_len <= cap {
        return None;
    }
    Some(
        enforce_sectioned_budget(primary, optional, cap, &|dropped, truncated| {
            file_context_continuation(path, sections, include_tests, dropped, truncated, total_len)
        })
        .text,
    )
}

/// The same for a `get_symbol_context` trace answer.
#[allow(clippy::too_many_arguments)] // every argument that selected the symbol travels
pub(crate) fn cap_default_symbol_context(
    path: &str,
    name: &str,
    symbol_kind: Option<&str>,
    symbol_line: Option<u32>,
    sections: Option<&[String]>,
    include_tests: bool,
    primary: &str,
    optional: &[(&str, &str)],
) -> Option<String> {
    let cap = super::file_read::GET_FILE_CONTENT_MAX_BYTES;
    let total_len = primary.len()
        + optional
            .iter()
            .map(|(_, body)| body.len() + 2)
            .sum::<usize>();
    if total_len <= cap {
        return None;
    }
    Some(
        enforce_sectioned_budget(primary, optional, cap, &|dropped, truncated| {
            symbol_context_continuation(
                path,
                name,
                symbol_kind,
                symbol_line,
                sections,
                include_tests,
                dropped,
                truncated,
                total_len,
            )
        })
        .text,
    )
}

/// Rewrite a stamped `full` completeness claim to `budget-limited` after a
/// post-assembly truncation invalidated it. Handles both envelope forms: the
/// compact one-liner (`Trust: <match> | <authority> | <parse> | full...`) and
/// the expanded `Completeness: full...` line. Only the envelope region (the
/// leading lines) is rewritten; body text is never touched.
pub fn downgrade_full_completeness_after_truncation(output: &str) -> String {
    let mut lines: Vec<String> = output.lines().map(str::to_string).collect();
    for line in lines.iter_mut().take(6) {
        if let Some(rest) = line.strip_prefix("Trust: ") {
            let mut parts: Vec<&str> = rest.splitn(4, " | ").collect();
            if parts.len() == 4 && parts[3].starts_with("full") {
                let downgraded = format!("budget-limited (was: {})", parts[3]);
                parts[3] = &downgraded;
                *line = format!("Trust: {}", parts.join(" | "));
            }
        } else if let Some(rest) = line.strip_prefix("Completeness: ")
            && rest.starts_with("full")
        {
            *line = format!("Completeness: budget-limited (was: {rest})");
        }
    }
    let mut rewritten = lines.join("\n");
    if output.ends_with('\n') {
        rewritten.push('\n');
    }
    rewritten
}

pub(crate) fn truncate_text_at_line_boundary(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }

    let mut last_char_end = 0usize;
    let mut last_newline_end = None;
    for (idx, ch) in text.char_indices() {
        let char_end = idx + ch.len_utf8();
        if char_end > max_bytes {
            break;
        }
        last_char_end = char_end;
        if ch == '\n' {
            last_newline_end = Some(char_end);
        }
    }

    let end = last_newline_end.unwrap_or(last_char_end);
    text[..end].to_string()
}

/// Session repeat-read cache-hit body (011 US1). Shared by full tools and STEL.
pub fn format_session_cache_hit_body(
    meta: &super::session::SessionCacheHitMeta,
    decision_reason: &str,
) -> String {
    let cache = serde_json::json!({
        "kind": meta.kind,
        "path": meta.path,
        "name": meta.name,
        "prior_tokens": meta.prior_tokens,
        "session_age_secs": meta.session_age_secs,
        "retrieve_handle": meta.retrieve_handle,
    });
    let json = serde_json::to_string_pretty(&cache).expect("cache payload serializes");
    let target = if meta.name.is_empty() {
        format!("file `{}`", meta.path)
    } else {
        format!("symbol `{}` in `{}`", meta.name, meta.path)
    };
    format!(
        "Decision: cache_hit\n\
         Economics: cache_hit ({decision_reason})\n\
         Session cache: {} {target} (prior_tokens={}, session_age_secs={})\n\
         \n\
         SymForge did not re-execute the read for this request.\n\
         These bytes were served earlier on this MCP connection; that is not proof they are in your context.\n\
         retrieve: symforge_retrieve with hash=\"{}\"\n\
         force_refresh=true re-reads the live index and is not the recovery path for missing bytes.\n\
         \n\
         --- cache payload ---\n\
         {json}",
        meta.kind, meta.prior_tokens, meta.session_age_secs, meta.retrieve_handle,
    )
}

#[cfg(test)]
mod sectioned_budget_tests {
    use super::*;

    const NEXT: &str = "tool(arg=1)";

    fn next(_: &[String], _: bool) -> String {
        NEXT.to_string()
    }

    #[test]
    fn everything_fits_is_returned_unchanged() {
        let out = enforce_sectioned_budget("primary", &[("x", "xx"), ("y", "yy")], 1000, &next);
        assert_eq!(out.text, "primary\n\nxx\n\nyy");
        assert!(out.dropped.is_empty() && !out.truncated);
    }

    #[test]
    fn optional_sections_drop_first_in_declared_order_and_are_counted() {
        let primary = "p".repeat(200);
        let optional = [("git", "g".repeat(300)), ("types", "t".repeat(300))];
        let borrowed: Vec<(&str, &str)> = optional.iter().map(|(n, b)| (*n, b.as_str())).collect();
        let out = enforce_sectioned_budget(&primary, &borrowed, 700, &next);
        assert_eq!(out.dropped, vec!["git".to_string()]);
        assert!(!out.truncated);
        assert!(out.text.starts_with(&primary));
        assert!(out.text.contains(&"t".repeat(300)) && !out.text.contains(&"g".repeat(300)));
        assert!(out.text.contains("1 optional section(s): git") && out.text.contains(NEXT));
        assert!(
            out.text.len() <= 700,
            "footer must fit the limit: {}",
            out.text.len()
        );
    }

    #[test]
    fn primary_is_cut_only_when_it_alone_exceeds_the_limit() {
        let primary = "line\n".repeat(1000);
        let out = enforce_sectioned_budget(&primary, &[("opt", "extra")], 600, &next);
        assert!(out.truncated);
        assert_eq!(out.dropped, vec!["opt".to_string()]);
        assert!(out.text.starts_with("line\nline\n"));
        assert!(out.text.contains("tail of the primary content") && out.text.contains(NEXT));
        assert!(!out.text.contains("extra"));
        assert!(
            out.text.len() <= 600,
            "footer must fit the limit: {}",
            out.text.len()
        );
    }

    #[test]
    fn a_cut_answer_does_not_keep_a_full_completeness_claim() {
        let primary = format!(
            "Trust: exact | current index | parsed | full\nScope: s\n\n{}",
            "body\n".repeat(500)
        );
        let out = enforce_sectioned_budget(&primary, &[], 400, &next);
        assert!(out.truncated);
        let first = out.text.lines().next().unwrap_or_default();
        assert!(
            first.contains("budget-limited") && !first.ends_with("| full"),
            "{first}"
        );
    }
}

#[cfg(test)]
mod continuation_tests {
    use super::*;

    #[test]
    fn continuations_carry_every_selecting_argument() {
        let read = symbol_read_continuation("src/a.rs", "twin", Some("method"), Some(12), 100_000);
        assert!(
            read.contains("kind=\"method\"") && read.contains("symbol_line=12"),
            "{read}"
        );
        let dropped = vec!["knowledge".to_string()];
        let asked = vec!["dependents".to_string()];
        let trace = symbol_context_continuation(
            "src/a.rs",
            "twin",
            Some("method"),
            Some(12),
            Some(&asked),
            true,
            &dropped,
            true,
            100_000,
        );
        assert_eq!(
            trace.matches("symbol_kind=\"method\"").count(),
            2,
            "{trace}"
        );
        assert_eq!(trace.matches("symbol_line=12").count(), 2, "{trace}");
        assert_eq!(trace.matches("include_tests=true").count(), 2, "{trace}");
        // The caller's own sections come back, never a widened `[]`.
        assert!(
            trace.contains("sections=[\"knowledge\"]")
                && trace.contains("sections=[\"dependents\"], max_tokens=")
                && !trace.contains("sections=[]"),
            "{trace}"
        );
    }

    #[test]
    fn file_context_continuation_carries_include_tests() {
        let dropped = vec!["knowledge".to_string()];
        let calls = file_context_continuation("src/a.rs", None, true, &dropped, true, 100_000);
        assert_eq!(calls.matches("include_tests=true").count(), 2, "{calls}");
        let none = file_context_continuation("src/a.rs", None, false, &dropped, true, 100_000);
        assert!(!none.contains("include_tests"), "{none}");
    }
}
