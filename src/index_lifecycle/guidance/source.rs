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
const APPROX_BYTES_PER_TOKEN: u64 = 4;

pub(crate) fn approx_tokens_from_bytes(bytes: usize) -> u64 {
    ((bytes as u64).saturating_add(APPROX_BYTES_PER_TOKEN - 1)) / APPROX_BYTES_PER_TOKEN
}

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
    let actual_tokens_est = approx_tokens_from_bytes(output.len());
    let mut truncated = truncate_text_at_line_boundary(&output, max_bytes);
    let detail =
        format!("Original output is ~{actual_tokens_est} tokens; budget is {max_tokens} tokens.");
    truncated.push_str(&token_truncation_footer(max_tokens, Some(detail.as_str())));
    (truncated, true)
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
