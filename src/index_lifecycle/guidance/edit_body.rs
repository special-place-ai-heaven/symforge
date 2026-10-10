//! The MCP edit tools' answer text, shared by the MCP handlers and every
//! embedded edit lane: the safety envelope, the per-operation summary, the
//! stale-reference warnings, the tee snapshot hint, the `working_directory`
//! reroute report, the project-config trust suffix and the impact footer, in
//! the order each MCP tool appends them.

use std::path::Path;

use crate::domain::index::{LanguageId, SymbolRecord};
use crate::edit_safety::trust::{ProjectConfigTrust, TrustEvaluation, TrustStatus};
use crate::live_index::LiveIndex;
use crate::live_index::query::{
    SymbolSelectorMatch, render_symbol_selector, resolve_symbol_selector,
};
use crate::live_index::store::IndexedFile;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EditSafetyMode {
    StructuralEditSafe,
    TextEditSafe,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EditSourceAuthority {
    DiskRefreshed,
    CurrentIndex,
    /// The edit base was re-read and re-parsed from the rerouted worktree
    /// TARGET because it had diverged from the indexed copy (a prior routed
    /// edit). Splicing into index content here would silently discard those
    /// earlier routed edits (review finding 5, post-v7.19.0).
    WorktreeTarget,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EditWriteSemantics {
    DryRunNoWrites,
    AtomicWriteAndReindex,
    TransactionalWriteRollbackAndReindex,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MatchType {
    Exact,
    Constrained,
}

fn safety_mode_label(mode: EditSafetyMode) -> &'static str {
    match mode {
        EditSafetyMode::StructuralEditSafe => "structural-edit-safe",
        EditSafetyMode::TextEditSafe => "text-edit-safe",
    }
}

fn source_authority_label(authority: EditSourceAuthority) -> &'static str {
    match authority {
        EditSourceAuthority::DiskRefreshed => "disk-refreshed",
        EditSourceAuthority::CurrentIndex => "current index",
        EditSourceAuthority::WorktreeTarget => "worktree target (rebased)",
    }
}

fn write_semantics_label(semantics: EditWriteSemantics) -> &'static str {
    match semantics {
        EditWriteSemantics::DryRunNoWrites => "dry run (no writes)",
        EditWriteSemantics::AtomicWriteAndReindex => "atomic write + reindex",
        EditWriteSemantics::TransactionalWriteRollbackAndReindex => {
            "transactional write + rollback + reindex"
        }
    }
}

fn match_type_label(match_type: MatchType) -> &'static str {
    match match_type {
        MatchType::Exact => "exact",
        MatchType::Constrained => "constrained",
    }
}

pub(crate) fn format_edit_envelope(
    safety_mode: EditSafetyMode,
    source_authority: EditSourceAuthority,
    write_semantics: EditWriteSemantics,
    evidence_anchor: &str,
) -> String {
    format!(
        "Edit safety: {}\nPath authority: repository-bound\nSource authority: {}\nWrite semantics: {}\nEvidence: symbol anchor `{}`",
        safety_mode_label(safety_mode),
        source_authority_label(source_authority),
        write_semantics_label(write_semantics),
        evidence_anchor
    )
}

pub(crate) fn format_batch_envelope(
    safety_mode: EditSafetyMode,
    match_type: MatchType,
    source_authority: EditSourceAuthority,
    write_semantics: EditWriteSemantics,
    evidence: &str,
) -> String {
    format!(
        "Edit safety: {}\nMatch type: {}\nPath authority: repository-bound\nSource authority: {}\nWrite semantics: {}\nEvidence: {}",
        safety_mode_label(safety_mode),
        match_type_label(match_type),
        source_authority_label(source_authority),
        write_semantics_label(write_semantics),
        evidence
    )
}

/// Format the result of a replace_symbol_body operation.
pub(crate) fn format_replace(
    path: &str,
    name: &str,
    kind: &str,
    old_bytes: usize,
    new_bytes: usize,
) -> String {
    format!("{path} — replaced {kind} `{name}` ({old_bytes} → {new_bytes} bytes)")
}

/// Format the result of an insert operation.
pub(crate) fn format_insert(
    path: &str,
    name: &str,
    position: &str,
    inserted_bytes: usize,
) -> String {
    format!("{path} — inserted {position} `{name}` ({inserted_bytes} bytes)")
}

/// Format the result of a delete operation.
pub(crate) fn format_delete(path: &str, name: &str, kind: &str, deleted_bytes: usize) -> String {
    format!("{path} — deleted {kind} `{name}` ({deleted_bytes} bytes)")
}

/// Format the result of an edit-within-symbol operation.
pub(crate) fn format_edit_within(
    path: &str,
    name: &str,
    replacements: usize,
    old_bytes: usize,
    new_bytes: usize,
) -> String {
    format!(
        "{path} — edited within `{name}` ({replacements} replacement(s), {old_bytes} → {new_bytes} bytes)"
    )
}

/// Format stale reference warnings after a signature-changing edit.
pub(crate) fn format_stale_warnings(
    _path: &str,
    name: &str,
    refs: &[(String, u32, Option<String>)],
) -> String {
    if refs.is_empty() {
        return String::new();
    }
    let mut out = format!(
        "\n[!] Signature of `{name}` may have changed — {} reference(s) to check:\n",
        refs.len()
    );
    for (ref_path, line, enclosing) in refs {
        out.push_str(&format!("  {ref_path}:{line}"));
        if let Some(enc) = enclosing {
            out.push_str(&format!(" (in {enc})"));
        }
        out.push('\n');
    }
    out
}

/// Format a batch edit summary.
pub(crate) fn format_batch_summary(results: &[String], file_count: usize) -> String {
    let mut out = format!("{} edit(s) across {} file(s):\n", results.len(), file_count);
    for r in results {
        out.push_str("  ");
        out.push_str(r);
        out.push('\n');
    }
    out
}

pub(crate) fn append_response_suffix_to_first_summary(summaries: &mut Vec<String>, suffix: &str) {
    let suffix = suffix.trim_start_matches('\n');
    if suffix.is_empty() {
        return;
    }
    let indented = suffix
        .lines()
        .map(|line| format!("  {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    if let Some(first) = summaries.first_mut() {
        first.push('\n');
        first.push_str(&indented);
    } else {
        summaries.push(indented);
    }
}

// ---------------------------------------------------------------------------
// Stale reference detection
// ---------------------------------------------------------------------------

/// Extract the first line of a symbol as a rough "signature" for change detection.
pub(crate) fn extract_signature(content: &[u8], byte_range: (u32, u32)) -> String {
    let start = byte_range.0 as usize;
    let end = byte_range.1 as usize;
    let slice = &content[start..end];
    let first_line_end = slice
        .iter()
        .position(|&b| b == b'\n')
        .unwrap_or(slice.len());
    String::from_utf8_lossy(&slice[..first_line_end]).to_string()
}

/// Detect references that may be stale after a symbol edit.
/// Compares old vs new signature (first line). Returns (path, line, enclosing_name) triples.
///
/// When `parent_type` is provided (i.e. the symbol is a method inside an `impl` block),
/// only warns about references in files that also mention the parent type — this avoids
/// false positives like warning about `Path::display()` when `Widget::display()` changed.
pub(crate) fn stale_references(
    guard: &LiveIndex,
    path: &str,
    name: &str,
    old_signature: &str,
    new_signature: &str,
    parent_type: Option<&str>,
    source_language: Option<&LanguageId>,
) -> Vec<(String, u32, Option<String>)> {
    if old_signature == new_signature {
        return Vec::new();
    }
    let refs = guard.find_references_for_name(name, None, false);

    // When we know the parent type, collect the set of files that reference it.
    // Only those files could plausibly call `ParentType::method_name()`.
    let type_files: Option<std::collections::HashSet<&str>> = parent_type.map(|tn| {
        guard
            .find_references_for_name(tn, None, false)
            .into_iter()
            .map(|(fp, _)| fp)
            .collect()
    });

    refs.into_iter()
        .filter(|(ref_path, _)| *ref_path != path)
        .filter(|(ref_path, _)| {
            // Skip references in files of a different language to reduce false positives
            // (e.g., Rust `add` flagging Python's `add`).
            if let Some(lang) = source_language
                && let Some(ref_file) = guard.get_file(ref_path)
                && ref_file.language != *lang
            {
                return false;
            }
            true
        })
        .filter(|(ref_path, _)| {
            // If we have a parent type filter, only keep refs in files that also mention it.
            match &type_files {
                Some(tf) => tf.contains(ref_path),
                None => true,
            }
        })
        .map(|(ref_path, rr)| {
            let enclosing = rr.enclosing_symbol_index.and_then(|idx| {
                guard
                    .get_file(ref_path)
                    .and_then(|f| f.symbols.get(idx as usize))
                    .map(|s| s.name.clone())
            });
            (ref_path.to_string(), rr.line_range.0 + 1, enclosing)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Impact footer
// ---------------------------------------------------------------------------

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

/// The impact footer text for `path` over one publication: its dependents and,
/// when `temporal` is Ready, its top co-change partners.
pub(crate) fn impact_footer_for(
    index: &LiveIndex,
    temporal: &crate::live_index::git_temporal::GitTemporalIndex,
    path: &str,
) -> String {
    let (deps, cochanges) = super::edit_plan::edit_impact_summary(index, temporal, path);
    impact_footer(deps, &cochanges)
}

// ---------------------------------------------------------------------------
// Project-config trust suffix
// ---------------------------------------------------------------------------

pub(crate) const PROJECT_CONFIG_TRUST_MODE_ENV: &str = "SYMFORGE_PROJECT_CONFIG_TRUST_MODE";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectConfigTrustMode {
    LogOnly,
    Enforce,
}

impl ProjectConfigTrustMode {
    pub(crate) fn current() -> Self {
        match std::env::var(PROJECT_CONFIG_TRUST_MODE_ENV) {
            Ok(value) if value.eq_ignore_ascii_case("enforce") => Self::Enforce,
            _ => Self::LogOnly,
        }
    }
}

pub(crate) fn project_config_trust_inputs_exist(repo_root: &Path) -> bool {
    let symforge_dir = repo_root.join(".symforge");
    symforge_dir.join("config.toml").exists() || symforge_dir.join("config").exists()
}

/// The suffix an edit answer carries for `repo_root`'s project config, or the
/// enforced refusal. `store` resolves the trust store only when the project
/// carries config inputs; `evaluate` reads it.
pub(crate) fn project_config_trust_response_suffix(
    repo_root: &Path,
    store: impl FnOnce() -> Option<ProjectConfigTrust>,
    evaluate: impl FnOnce(&ProjectConfigTrust) -> TrustEvaluation,
    mode: impl FnOnce() -> ProjectConfigTrustMode,
) -> Result<Option<String>, String> {
    if !project_config_trust_inputs_exist(repo_root) {
        return Ok(None);
    }
    let Some(trust) = store() else {
        return Ok(Some(
            "ProjectConfigTrustWarning: status=Unavailable warning=\"could not determine user-local data directory\"; mode=LOG_ONLY; operation_allowed=true"
                .to_string(),
        ));
    };
    let evaluation = evaluate(&trust);
    match evaluation.status {
        TrustStatus::Trusted | TrustStatus::EnvOverride => Ok(None),
        TrustStatus::Untrusted | TrustStatus::ContentChanged { .. } => {
            let evidence = project_config_trust_evidence(&evaluation);
            match mode() {
                ProjectConfigTrustMode::LogOnly => Ok(Some(format!(
                    "ProjectConfigTrustWarning: {evidence}; mode=LOG_ONLY; operation_allowed=true"
                ))),
                ProjectConfigTrustMode::Enforce => Err(format!(
                    "ProjectConfigTrustEnforced: {evidence}; mode=ENFORCE; operation_allowed=false; run `symforge trust project-config accept --project {}` with reviewed actual_hash before retrying",
                    repo_root.display()
                )),
            }
        }
    }
}

fn project_config_trust_evidence(evaluation: &TrustEvaluation) -> String {
    let mut parts = match &evaluation.status {
        TrustStatus::Trusted => vec!["status=Trusted".to_string()],
        TrustStatus::Untrusted => vec!["status=Untrusted".to_string()],
        TrustStatus::ContentChanged { expected, actual } => vec![
            "status=ContentChanged".to_string(),
            format!("expected_hash={expected}"),
            format!("actual_hash={actual}"),
        ],
        TrustStatus::EnvOverride => vec!["status=EnvOverride".to_string()],
    };
    if !matches!(evaluation.status, TrustStatus::ContentChanged { .. }) {
        parts.push(format!("actual_hash={}", evaluation.actual_hash));
    }
    if let Some(project_key) = &evaluation.project_key {
        parts.push(format!("project_key={project_key}"));
    }
    if let Some(warning) = evaluation.warnings.first() {
        parts.push(format!("warning=\"{}\"", one_line(warning)));
    }
    parts.join(" ")
}

fn one_line(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn append_project_config_trust_suffix(output: &mut String, suffix: Option<&str>) {
    if let Some(suffix) = suffix {
        output.push('\n');
        output.push_str(suffix);
    }
}

// ---------------------------------------------------------------------------
// Answer composition
// ---------------------------------------------------------------------------

/// One single-symbol edit answer (`replace_symbol_body`, `insert_symbol`,
/// `delete_symbol`, `edit_within_symbol`), in MCP's append order. A dry run
/// carries no stale warnings, tee hint, reroute report or impact footer.
pub(crate) struct SingleEditAnswer<'a> {
    pub safety: EditSafetyMode,
    pub authority: EditSourceAuthority,
    pub semantics: EditWriteSemantics,
    pub anchor: &'a str,
    /// The operation summary, with any targeting note already appended.
    pub summary: &'a str,
    /// [`format_stale_warnings`] output.
    pub stale_warnings: &'a str,
    /// `format_tee_snapshot_suffix` output.
    pub tee: &'a str,
    /// The reroute report (`edit_route::format_reroute_suffix`).
    pub reroute: &'a str,
    /// `replace_symbol_body` appends the tee hint before the reroute report;
    /// the other three tools append it after.
    pub tee_before_reroute: bool,
    pub trust: Option<&'a str>,
    /// The impact footer text, without its leading newline.
    pub impact: Option<&'a str>,
}

pub(crate) fn render_single_edit_answer(answer: &SingleEditAnswer<'_>) -> String {
    let mut out = format!(
        "{}\n{}",
        format_edit_envelope(
            answer.safety,
            answer.authority,
            answer.semantics,
            answer.anchor
        ),
        answer.summary
    );
    out.push_str(answer.stale_warnings);
    if answer.tee_before_reroute {
        out.push_str(answer.tee);
        out.push_str(answer.reroute);
    } else {
        out.push_str(answer.reroute);
        out.push_str(answer.tee);
    }
    append_project_config_trust_suffix(&mut out, answer.trust);
    if let Some(impact) = answer.impact {
        out.push('\n');
        out.push_str(impact);
    }
    out
}

/// One batch answer (`batch_edit`, `batch_insert`, `batch_rename`): the batch
/// envelope, the summary body, the trust suffix and the primary path's
/// impact footer.
pub(crate) struct BatchEditAnswer<'a> {
    pub match_type: MatchType,
    pub authority: EditSourceAuthority,
    pub semantics: EditWriteSemantics,
    pub evidence: &'a str,
    pub body: &'a str,
    pub trust: Option<&'a str>,
    pub impact: Option<&'a str>,
}

pub(crate) fn render_batch_edit_answer(answer: &BatchEditAnswer<'_>) -> String {
    let mut out = format!(
        "{}\n{}",
        format_batch_envelope(
            EditSafetyMode::StructuralEditSafe,
            answer.match_type,
            answer.authority,
            answer.semantics,
            answer.evidence,
        ),
        answer.body,
    );
    append_project_config_trust_suffix(&mut out, answer.trust);
    if let Some(impact) = answer.impact {
        out.push('\n');
        out.push_str(impact);
    }
    out
}

/// The single-edit dry-run summary MCP prints for each tool.
pub(crate) fn dry_run_replace_summary(
    name: &str,
    path: &str,
    old_bytes: usize,
    new_bytes: usize,
) -> String {
    format!(
        "[DRY RUN] Would replace `{name}` in {path} (old: {old_bytes} bytes -> new: {new_bytes} bytes)"
    )
}

pub(crate) fn dry_run_insert_summary(
    position: &str,
    name: &str,
    path: &str,
    content_bytes: usize,
) -> String {
    format!(
        "[DRY RUN] Would insert {position} `{name}` in {path} ({content_bytes} bytes of content)"
    )
}

pub(crate) fn dry_run_delete_summary(name: &str, path: &str, deleted_bytes: usize) -> String {
    format!("[DRY RUN] Would delete `{name}` in {path} ({deleted_bytes} bytes)")
}

/// `edit_within_symbol`'s summary line plus its first-of-several note.
pub(crate) fn edit_within_summary(
    dry_run: bool,
    path: &str,
    name: &str,
    replacements: usize,
    old_bytes: usize,
    new_bytes: usize,
    untargeted_extra: usize,
) -> String {
    let mut out = if dry_run {
        format!("[DRY RUN] Would edit within `{name}` in {path} ({replacements} replacement(s))")
    } else {
        format_edit_within(path, name, replacements, old_bytes, new_bytes)
    };
    if untargeted_extra > 0 {
        if dry_run {
            out.push_str(&format!(
                "\nNote: `old_text` occurs {} times within `{}`; this targets the FIRST. Pass `occurrence: N` or `near_line: L` to pick another.",
                untargeted_extra + 1,
                name
            ));
        } else {
            // Dogfood #4: replacing the first of several matches silently is a
            // mini trust lie — disclose the ambiguity and the targeting knobs.
            out.push_str(&format!(
                "\nNote: `old_text` occurred {} times within `{}`; edited the FIRST. Pass `occurrence: N` or `near_line: L` to target another.",
                untargeted_extra + 1,
                name
            ));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Symbol resolution and edit-within refusals
// ---------------------------------------------------------------------------

const MAX_SYMBOL_SUGGESTIONS: usize = 3;
const MAX_SYMBOL_SUGGESTION_DISTANCE: usize = 3;
const MIN_SYMBOL_SUGGESTION_CONFIDENCE: f64 = 0.6;

fn did_you_mean_suffix(file: &IndexedFile, requested: &str) -> String {
    let suggestions = same_file_symbol_suggestions(file, requested);
    if suggestions.is_empty() {
        String::new()
    } else {
        format!(" did_you_mean: [{}]", suggestions.join(", "))
    }
}

fn same_file_symbol_suggestions(file: &IndexedFile, requested: &str) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    let mut scored = Vec::new();

    for sym in &file.symbols {
        let candidate = sym.name.trim();
        if candidate.is_empty() || candidate == requested || !seen.insert(candidate.to_string()) {
            continue;
        }

        if let Some((score, distance)) = symbol_suggestion_score(requested, candidate) {
            scored.push((score, distance, sym.line_range.0, candidate.to_string()));
        }
    }

    scored.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.2.cmp(&b.2))
            .then_with(|| a.3.cmp(&b.3))
    });

    scored
        .into_iter()
        .take(MAX_SYMBOL_SUGGESTIONS)
        .map(|(_, _, _, name)| name)
        .collect()
}

fn symbol_suggestion_score(requested: &str, candidate: &str) -> Option<(u16, usize)> {
    let requested_norm = normalize_symbol_name(requested);
    let candidate_norm = normalize_symbol_name(candidate);
    if requested_norm.is_empty() || candidate_norm.is_empty() {
        return None;
    }

    let mut best = bounded_levenshtein(
        &requested_norm,
        &candidate_norm,
        MAX_SYMBOL_SUGGESTION_DISTANCE,
    )
    .and_then(|distance| {
        let max_len = requested_norm
            .chars()
            .count()
            .max(candidate_norm.chars().count());
        let confidence = 1.0 - (distance as f64 / max_len as f64);
        if confidence >= MIN_SYMBOL_SUGGESTION_CONFIDENCE {
            Some(((confidence * 1000.0) as u16, distance))
        } else {
            None
        }
    });

    if has_separator_prefix(requested, candidate) {
        let prefix_score = 900;
        let prefix_distance = bounded_levenshtein(
            &requested_norm,
            &candidate_norm,
            MAX_SYMBOL_SUGGESTION_DISTANCE,
        )
        .unwrap_or(MAX_SYMBOL_SUGGESTION_DISTANCE + 1);
        match best {
            Some((score, _)) if score >= prefix_score => {}
            _ => best = Some((prefix_score, prefix_distance)),
        }
    }

    best
}

fn normalize_symbol_name(name: &str) -> String {
    let mut normalized = String::new();
    for ch in name.chars() {
        for folded in ch.to_lowercase() {
            if folded.is_alphanumeric() {
                normalized.push(folded);
            }
        }
    }
    normalized
}

fn has_separator_prefix(requested: &str, candidate: &str) -> bool {
    if normalize_symbol_name(requested).chars().count() < 3 {
        return false;
    }

    let requested = requested.to_lowercase();
    let candidate = candidate.to_lowercase();
    let mut candidate_chars = candidate.chars();
    for requested_char in requested.chars() {
        if candidate_chars.next() != Some(requested_char) {
            return false;
        }
    }

    matches!(candidate_chars.next(), Some(ch) if !ch.is_alphanumeric())
}

fn bounded_levenshtein(left: &str, right: &str, max_distance: usize) -> Option<usize> {
    let left_chars = left.chars().collect::<Vec<_>>();
    let right_chars = right.chars().collect::<Vec<_>>();
    if left_chars.len().abs_diff(right_chars.len()) > max_distance {
        return None;
    }

    let mut previous = (0..=right_chars.len()).collect::<Vec<_>>();
    let mut current = vec![0; right_chars.len() + 1];

    for (left_index, left_char) in left_chars.iter().enumerate() {
        current[0] = left_index + 1;
        for (right_index, right_char) in right_chars.iter().enumerate() {
            let deletion = previous[right_index + 1] + 1;
            let insertion = current[right_index] + 1;
            let substitution = previous[right_index] + usize::from(left_char != right_char);
            current[right_index + 1] = deletion.min(insertion).min(substitution);
        }
        std::mem::swap(&mut previous, &mut current);
    }

    let distance = previous[right_chars.len()];
    (distance <= max_distance).then_some(distance)
}

/// Resolve a symbol by name/kind/line, returning (index, cloned record) or user-friendly error.
pub(crate) fn resolve_or_error(
    file: &IndexedFile,
    name: &str,
    kind: Option<&str>,
    line: Option<u32>,
) -> Result<(usize, SymbolRecord), String> {
    match resolve_symbol_selector(file, name, kind, line) {
        SymbolSelectorMatch::Selected(idx, sym) => Ok((idx, sym.clone())),
        SymbolSelectorMatch::NotFound => {
            let label = render_symbol_selector(name, kind, line);
            // Surface parse status so users know WHY symbols are missing.
            let status_hint = match &file.parse_status {
                crate::live_index::store::ParseStatus::Failed { error } => {
                    format!(
                        " (file failed to parse: {error} — symbol tools unavailable for this file)"
                    )
                }
                crate::live_index::store::ParseStatus::PartialParse { warning } => {
                    format!(
                        " (file partially parsed with errors: {warning} — some symbols may be missing)"
                    )
                }
                _ => String::new(),
            };
            let suggestion_hint = did_you_mean_suffix(file, name);
            Err(format!(
                "Symbol not found: {label}{status_hint}{suggestion_hint}"
            ))
        }
        SymbolSelectorMatch::Ambiguous(candidate_lines) => {
            let candidates = candidate_lines
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            Err(format!(
                "Ambiguous: multiple definitions of `{name}`. \
                 Pass `symbol_line` to disambiguate. Candidate lines: {candidates}"
            ))
        }
    }
}

/// MCP `edit_within_symbol`'s text for a refused text selection inside
/// `name`, whose body is `body`. Source text in the message is withheld when
/// the secret guard refuses it.
pub(crate) fn edit_within_refusal(
    error: crate::edit_safety::structural::WithinSelectionError,
    body_str: &str,
    old_text: &str,
    name: &str,
) -> String {
    use crate::edit_safety::structural::WithinSelectionError;
    let output = match error {
        WithinSelectionError::InvalidSpan | WithinSelectionError::InvalidUtf8 =>
            "Error: symbol body is not valid UTF-8.".to_string(),
        WithinSelectionError::ConflictingTargeting =>
            "Error: `replace_all`, `occurrence`, and `near_line` are mutually exclusive — pass at most one targeting mode.".to_string(),
        WithinSelectionError::OccurrenceOutOfRange { requested, total } =>
            format!("Error: occurrence {requested} is out of range — `old_text` has {total} exact occurrence(s) within `{name}`."),
        WithinSelectionError::NotFound => {
            let preview_len = if body_str.len() <= 800 {
                body_str.len()
            } else {
                body_str.char_indices()
                    .map(|(offset, _)| offset)
                    .take_while(|offset| *offset <= 800)
                    .last()
                    .unwrap_or(0)
            };
            let preview = &body_str[..preview_len];
            let truncated = if preview_len < body_str.len() {
                format!("\n... ({} more bytes)", body_str.len() - preview_len)
            } else {
                String::new()
            };
            let candidate = format!(
                "Error: `{}` not found within symbol `{}`. The symbol body is ({} bytes):\n```\n{}{}\n```",
                old_text, name, body_str.len(), preview, truncated,
            );
            if crate::knowledge::guard_query(&candidate).is_ok() {
                candidate
            } else {
                "Error: edit text not found; source preview withheld by safety policy.".to_string()
            }
        }
    };
    if crate::knowledge::guard_query(&output).is_ok() {
        output
    } else {
        "Error: edit target rejected by safety policy.".to_string()
    }
}
