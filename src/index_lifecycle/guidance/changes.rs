//! Source-independent change filtering and symbol-delta analysis.
use super::symbol_context::extract_declaration_name;
use std::collections::HashMap;

#[derive(Clone)]
pub(crate) struct SymbolFileDelta {
    pub path: String,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub modified: Vec<String>,
    pub withheld: Option<String>,
}

#[derive(Clone)]
pub(crate) struct SymbolDiffView {
    pub base: String,
    pub target: String,
    pub changed_files: usize,
    pub files: Vec<SymbolFileDelta>,
    pub added_count: usize,
    pub removed_count: usize,
    pub modified_count: usize,
    pub withheld_count: usize,
}

pub(crate) fn capture_symbol_diff(
    base: &str,
    target: &str,
    paths: &[&str],
    mut read: impl FnMut(&str, &str) -> Result<Option<String>, String>,
) -> SymbolDiffView {
    let mut view = SymbolDiffView {
        base: base.into(),
        target: target.into(),
        changed_files: paths.len(),
        files: Vec::new(),
        added_count: 0,
        removed_count: 0,
        modified_count: 0,
        withheld_count: 0,
    };
    for path in paths {
        let mut delta = SymbolFileDelta {
            path: (*path).into(),
            added: Vec::new(),
            removed: Vec::new(),
            modified: Vec::new(),
            withheld: None,
        };
        let contents =
            read(base, path).and_then(|before| read(target, path).map(|after| (before, after)));
        let (before, after) = match contents {
            Ok(pair) => pair,
            Err(refusal) => {
                delta.withheld = Some(refusal);
                view.withheld_count += 1;
                view.files.push(delta);
                continue;
            }
        };
        let before = before.unwrap_or_default();
        let after = after.unwrap_or_default();
        let base_symbols = crate::parsing::extract_symbols_for_diff(&before, path)
            .unwrap_or_else(|| extract_symbol_signatures(&before));
        let target_symbols = crate::parsing::extract_symbols_for_diff(&after, path)
            .unwrap_or_else(|| extract_symbol_signatures(&after));
        let base_names: HashMap<&str, &str> = base_symbols
            .iter()
            .map(|(name, signature)| (name.as_str(), signature.as_str()))
            .collect();
        let target_names: HashMap<&str, &str> = target_symbols
            .iter()
            .map(|(name, signature)| (name.as_str(), signature.as_str()))
            .collect();
        for (name, signature) in &target_names {
            match base_names.get(name) {
                None => delta.added.push((*name).to_string()),
                Some(old) if old != signature => delta.modified.push((*name).to_string()),
                _ => {}
            }
        }
        for name in base_names.keys() {
            if !target_names.contains_key(name) {
                delta.removed.push((*name).to_string());
            }
        }
        delta.added.sort_unstable();
        delta.removed.sort_unstable();
        delta.modified.sort_unstable();
        view.added_count += delta.added.len();
        view.removed_count += delta.removed.len();
        view.modified_count += delta.modified.len();
        view.files.push(delta);
    }
    view
}

pub(crate) fn render_symbol_diff(
    view: &SymbolDiffView,
    compact: bool,
    summary_only: bool,
) -> String {
    let target_label = if view.target.is_empty() {
        "working tree"
    } else {
        &view.target
    };
    let mut lines = vec![
        format!("Symbol diff: {}...{target_label}", view.base),
        format!("{} files changed", view.changed_files),
        String::new(),
    ];
    let mut files_with_changes = 0;
    for file in &view.files {
        if let Some(refusal) = &file.withheld {
            lines.push(refusal.clone());
            lines.push(String::new());
            continue;
        }
        if file.added.is_empty() && file.removed.is_empty() && file.modified.is_empty() {
            continue;
        }
        files_with_changes += 1;
        if summary_only {
            continue;
        }
        if compact {
            let mut parts = Vec::new();
            for (marker, names) in [
                ("+", &file.added),
                ("-", &file.removed),
                ("~", &file.modified),
            ] {
                if !names.is_empty() {
                    let names_list =
                        compact_symbol_list(&names.iter().map(String::as_str).collect::<Vec<_>>());
                    parts.push(format!("{marker}{}: {names_list}", names.len()));
                }
            }
            lines.push(format!("  {} ({})", file.path, parts.join(", ")));
        } else {
            lines.push(format!("── {} ──", file.path));
            for (marker, names) in [
                ("+", &file.added),
                ("-", &file.removed),
                ("~", &file.modified),
            ] {
                for name in names {
                    lines.push(format!("  {marker} {name}"));
                }
            }
            lines.push(String::new());
        }
    }
    lines.push(format!(
        "Summary: +{} added, -{} removed, ~{} modified",
        view.added_count, view.removed_count, view.modified_count,
    ));
    if view.added_count + view.removed_count + view.modified_count == 0 && view.changed_files > 0 {
        lines.push(format!(
            "Note: {} file(s) changed but no symbol boundaries were affected (changes in comments, whitespace, or non-symbol code).",
            view.changed_files,
        ));
    }
    if compact && files_with_changes > 0 && view.changed_files > files_with_changes {
        lines.push(format!(
            "({} file(s) with only non-symbol changes omitted)",
            view.changed_files - files_with_changes
        ));
    }
    lines.join("\n")
}

/// Format a list of symbol names for compact display: up to 3 names, then "..."
fn compact_symbol_list(names: &[&str]) -> String {
    let mut sorted: Vec<&str> = names.to_vec();
    sorted.sort_unstable();
    if sorted.len() <= 3 {
        sorted.join(", ")
    } else {
        format!("{}, ... +{} more", sorted[..3].join(", "), sorted.len() - 3)
    }
}

/// Extract symbol name → signature pairs from source code using simple pattern matching.
/// Returns Vec<(name, signature_line)> for functions, classes, structs, enums, traits, interfaces.
fn extract_symbol_signatures(content: &str) -> Vec<(String, String)> {
    let mut symbols = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        // Skip empty, comments, imports
        if trimmed.is_empty()
            || trimmed.starts_with("//")
            || trimmed.starts_with('#')
            || trimmed.starts_with("/*")
            || trimmed.starts_with('*')
            || trimmed.starts_with("use ")
            || trimmed.starts_with("import ")
            || trimmed.starts_with("from ")
        {
            continue;
        }

        // Match common symbol declaration patterns
        let name = extract_declaration_name(trimmed);
        if let Some(name) = name {
            symbols.push((name, trimmed.to_string()));
        }
    }
    symbols
}

// ── what_changed / diff_symbols request interpretation, shared by MCP and embed ──

use super::search_envelope::{SourceAuthority, format_search_envelope};

/// Which change set a `what_changed` call reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum WhatChangedMode {
    Timestamp(i64),
    GitRef(String),
    Uncommitted,
}

/// The caller-supplied options every `what_changed` adapter interprets the same way.
pub(crate) struct WhatChangedOptions<'a> {
    pub since: Option<i64>,
    pub git_ref: Option<&'a str>,
    pub uncommitted: Option<bool>,
    pub path_prefix: Option<&'a str>,
    pub language: Option<&'a str>,
    pub code_only: Option<bool>,
    pub include_symbol_diff: Option<bool>,
}

pub(crate) fn determine_what_changed_mode(
    input: &WhatChangedOptions<'_>,
    has_repo_root: bool,
) -> Result<WhatChangedMode, String> {
    if let Some(git_ref) = input
        .git_ref
        .map(str::trim)
        .filter(|git_ref| !git_ref.is_empty())
    {
        return if has_repo_root {
            Ok(WhatChangedMode::GitRef(
                git_ref
                    .strip_prefix("branch:")
                    .unwrap_or(git_ref)
                    .to_string(),
            ))
        } else {
            Err("Git change detection unavailable; pass `since` for timestamp mode.".to_string())
        };
    }

    if input.uncommitted.unwrap_or(false) || (input.since.is_none() && has_repo_root) {
        return if has_repo_root {
            Ok(WhatChangedMode::Uncommitted)
        } else {
            Err("Git change detection unavailable; pass `since` for timestamp mode.".to_string())
        };
    }

    if let Some(since) = input.since {
        Ok(WhatChangedMode::Timestamp(since))
    } else {
        Err(
            "what_changed requires either `since`, `git_ref`, or an available repo root."
                .to_string(),
        )
    }
}

pub(crate) fn filter_paths_by_prefix_and_language(
    paths: Vec<String>,
    path_prefix: Option<&str>,
    language: Option<&str>,
    code_only: bool,
) -> Result<Vec<String>, String> {
    let lang_filter = super::filters::parse_language_filter(language)?;
    let prefix = path_prefix
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| {
            p.replace('\\', "/")
                .trim_start_matches("./")
                .trim_start_matches('/')
                .trim_end_matches('/')
                .to_string()
        });

    Ok(paths
        .into_iter()
        .filter(|path| {
            if let Some(ref pfx) = prefix
                && !path.starts_with(pfx.as_str())
            {
                return false;
            }
            if let Some(ref lang) = lang_filter {
                let ext = path.rsplit('.').next().unwrap_or("");
                if crate::domain::index::LanguageId::from_extension(ext).as_ref() != Some(lang) {
                    return false;
                }
            }
            if code_only && lang_filter.is_none() {
                let ext = path.rsplit('.').next().unwrap_or("");
                match crate::domain::index::LanguageId::from_extension(ext) {
                    // Recovered finding #3: an unknown extension is not proof of
                    // "data" — SQL, shell, PowerShell, Proto, Terraform,
                    // Dockerfile, Makefile are legitimate source SymForge just
                    // cannot parse. Keep them under code_only.
                    None => return is_unparsed_source_path(path),
                    Some(lang) => {
                        if crate::parsing::config_extractors::is_config_language(&lang) {
                            return false;
                        }
                    }
                }
            }
            true
        })
        .collect())
}

/// Source formats with no `LanguageId` parser that are nonetheless
/// unambiguously source, not data — they must survive `code_only` filtering
/// (recovered finding #3). Deliberately small allowlist; extend as real
/// misclassifications surface.
fn is_unparsed_source_path(path: &str) -> bool {
    let file_name = path.rsplit('/').next().unwrap_or(path);
    let lower = file_name.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "dockerfile" | "makefile" | "gnumakefile" | "justfile"
    ) {
        return true;
    }
    let Some((_, ext)) = lower.rsplit_once('.') else {
        return false;
    };
    matches!(
        ext,
        "sql"
            | "sh"
            | "bash"
            | "zsh"
            | "ps1"
            | "psm1"
            | "psd1"
            | "bat"
            | "cmd"
            | "proto"
            | "tf"
            | "tfvars"
            | "cmake"
            | "gradle"
            | "dockerfile"
    )
}

/// The `diff_symbols` path filter: a raw (unnormalized) prefix, an exact
/// language match, and the code-only rule without the unparsed-source
/// allowlist that `what_changed` applies.
pub(crate) fn filter_diff_symbol_paths<'a>(
    paths: &'a [String],
    path_prefix: Option<&str>,
    language: Option<&str>,
    code_only: bool,
) -> Result<Vec<&'a str>, String> {
    let lang_filter = super::filters::parse_language_filter(language)?;
    Ok(paths
        .iter()
        .map(|s| s.as_str())
        .filter(|p| {
            if let Some(prefix) = path_prefix
                && !p.starts_with(prefix)
            {
                return false;
            }
            if let Some(ref lang) = lang_filter {
                let ext = p.rsplit('.').next().unwrap_or("");
                if crate::domain::index::LanguageId::from_extension(ext).as_ref() != Some(lang) {
                    return false;
                }
            }
            if code_only && lang_filter.is_none() {
                let ext = p.rsplit('.').next().unwrap_or("");
                match crate::domain::index::LanguageId::from_extension(ext) {
                    None => return false,
                    Some(lang) => {
                        if crate::parsing::config_extractors::is_config_language(&lang) {
                            return false;
                        }
                    }
                }
            }
            true
        })
        .collect())
}

pub(crate) fn changed_paths_completeness_label(
    before_filter: usize,
    after_filter: usize,
) -> String {
    if before_filter == after_filter {
        "full for current scope".to_string()
    } else {
        format!("full for filtered scope ({after_filter} of {before_filter} path(s) shown)")
    }
}

pub(crate) fn what_changed_scope_summary(
    input: &WhatChangedOptions<'_>,
    mode: &WhatChangedMode,
) -> String {
    let mut parts = Vec::new();
    match mode {
        WhatChangedMode::Timestamp(since_ts) => parts.push(format!("timestamp since `{since_ts}`")),
        WhatChangedMode::Uncommitted => parts.push("uncommitted working tree".to_string()),
        WhatChangedMode::GitRef(git_ref) => {
            parts.push(format!("git diff from `{git_ref}` to `HEAD`"))
        }
    }
    if let Some(path_prefix) = input.path_prefix.filter(|prefix| !prefix.trim().is_empty()) {
        parts.push(format!(
            "path prefix `{}`",
            super::file_search::normalize_exact_path(path_prefix)
        ));
    }
    if let Some(language) = input.language {
        parts.push(format!("language `{language}`"));
    }
    // US1 (018) III trust: disclose the code-only filter whenever it is
    // actually applied. Uncommitted mode now defaults it on (FR-001), so the
    // effective default is mode-scoped and must match the handler sites.
    let code_only_default = matches!(mode, WhatChangedMode::Uncommitted);
    if input.code_only.unwrap_or(code_only_default) {
        parts.push("code-only filter".to_string());
    }
    if input.include_symbol_diff.unwrap_or(false) {
        parts.push("symbol diff appended".to_string());
    }
    parts.join("; ")
}

pub(crate) fn what_changed_source_authority(
    mode: &WhatChangedMode,
    freshness: &crate::domain::FreshnessStatus,
) -> SourceAuthority {
    match mode {
        // T045: the Timestamp arm used to assert the literal "current index",
        // collapsing the envelope regardless of measured freshness — the same
        // forgeable-axis defect as the context lane, closed the same way.
        WhatChangedMode::Timestamp(_) => SourceAuthority::from_freshness(freshness),
        WhatChangedMode::Uncommitted => SourceAuthority::never_collapse("git working tree"),
        WhatChangedMode::GitRef(_) => SourceAuthority::never_collapse("git ref diff"),
    }
}

pub(crate) fn what_changed_parse_state_label(
    mode: &WhatChangedMode,
    include_symbol_diff: bool,
) -> &'static str {
    match mode {
        WhatChangedMode::Timestamp(_) => "parsed",
        _ if include_symbol_diff => {
            "degraded (git path diff + lexical symbol diff — regex extraction may miss nested symbols)"
        }
        _ => "not-applicable (git path diff)",
    }
}

const WHAT_CHANGED_MAX_PATHS: usize = 200;

pub(crate) fn what_changed_paths_result(paths: &[String], empty_message: &str) -> String {
    let mut normalized_paths: Vec<String> =
        paths.iter().map(|path| path.replace('\\', "/")).collect();
    normalized_paths.sort();
    normalized_paths.dedup();

    if normalized_paths.is_empty() {
        return empty_message.to_string();
    }

    let total = normalized_paths.len();
    if total > WHAT_CHANGED_MAX_PATHS {
        let omitted = total - WHAT_CHANGED_MAX_PATHS;
        normalized_paths.truncate(WHAT_CHANGED_MAX_PATHS);
        let mut out = normalized_paths.join("\n");
        out.push_str(&format!(
            "\n{} {WHAT_CHANGED_MAX_PATHS} of {total} paths shown; {omitted} more omitted.",
            super::source::CANONICAL_TRUNCATION_MARKER
        ));
        return out;
    }

    normalized_paths.join("\n")
}

pub(crate) fn what_changed_timestamp_view(
    view: &crate::live_index::WhatChangedTimestampView,
    since_ts: i64,
) -> String {
    if since_ts < view.loaded_secs {
        // Entire index is newer — list all files
        if view.paths.is_empty() {
            return "Index is empty — no files tracked.".to_string();
        }
        view.paths.join("\n")
    } else {
        "No changes detected since last index load.".to_string()
    }
}

pub(crate) fn search_paths_evidence<'a, I>(paths: I) -> String
where
    I: IntoIterator<Item = &'a str>,
{
    let anchors = paths
        .into_iter()
        .take(3)
        .map(std::borrow::ToOwned::to_owned)
        .collect();
    super::reference_read::anchored_search_evidence(anchors, "paths")
}

/// The measured envelope `diff_symbols` places above its symbol delta.
#[allow(clippy::too_many_arguments)]
pub(crate) fn diff_symbols_envelope(
    base: &str,
    target: &str,
    all_changed_files: usize,
    changed_files: &[&str],
    compact: bool,
    summary_only: bool,
    path_prefix: Option<&str>,
    language: Option<&str>,
    code_only: bool,
) -> String {
    let mut scope_parts = vec![format!("git diff `{base}`...`{target}`")];
    if let Some(path_prefix) = path_prefix.filter(|prefix| !prefix.trim().is_empty()) {
        scope_parts.push(format!(
            "path prefix `{}`",
            super::file_search::normalize_exact_path(path_prefix)
        ));
    }
    if let Some(language) = language {
        scope_parts.push(format!("language `{language}`"));
    }
    if code_only {
        scope_parts.push("code-only filter".to_string());
    }
    if compact {
        scope_parts.push("compact output".to_string());
    }
    if summary_only {
        scope_parts.push("summary-only output".to_string());
    }
    let completeness = if changed_files.len() == all_changed_files {
        "full for filtered git delta".to_string()
    } else {
        format!(
            "full for filtered git delta ({} of {} changed file(s) shown)",
            changed_files.len(),
            all_changed_files
        )
    };
    format_search_envelope(
        "exact (git ref diff)",
        SourceAuthority::never_collapse("git ref diff"),
        "high (tree-sitter AST extraction for supported languages, regex fallback for others)",
        &completeness,
        &scope_parts.join("; "),
        &search_paths_evidence(changed_files.iter().copied()),
    )
}

/// The `estimate=true` answer both `what_changed` adapters return before any work.
pub(crate) fn what_changed_estimate(include_symbol_diff: bool) -> (u64, String) {
    let est = if include_symbol_diff { 500 } else { 200 };
    (
        est,
        format!(
            "Estimate for what_changed: ~{est} tokens (include_symbol_diff={include_symbol_diff})"
        ),
    )
}

/// The `estimate=true` answer both `diff_symbols` adapters return before any work.
pub(crate) fn diff_symbols_estimate(compact: bool, summary_only: bool) -> (u64, String) {
    let est = if summary_only {
        50
    } else if compact {
        200
    } else {
        500
    };
    (
        est,
        format!(
            "Estimate for diff_symbols: ~{est} tokens (compact={compact}, summary_only={summary_only})"
        ),
    )
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
            "\n{} one or more lists capped; see `pagination` for full totals and returned counts.",
            super::source::CANONICAL_TRUNCATION_MARKER
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
