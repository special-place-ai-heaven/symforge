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

pub(crate) fn format_capability_warning(
    tool_name: &str,
    language: &str,
    required_safety: &str,
    available_safety: &str,
    suggestion: &str,
) -> String {
    format!(
        "{tool_name}: edit safety blocked\nRequired safety: {required_safety}\nAvailable safety: {available_safety}\nLanguage: {language}\nSuggested next step: {suggestion}"
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

/// Format the worktree-routing suffix appended to edit responses when the caller
/// supplied `working_directory`. Produces concise target evidence so agents can
/// verify the requested workspace, actual write target, indexed path, and
/// reroute state. Returns an empty string when `working_directory` was omitted,
/// preserving byte-identical output for today's callers.
pub(crate) fn format_reroute_suffix(
    working_directory: Option<&std::path::Path>,
    resolved: &crate::worktree::ResolvedTarget,
) -> String {
    let Some(working_directory) = working_directory else {
        return String::new();
    };
    format!(
        "\nworking_directory: {}\nrerouted: {}\nwrote_to: {}\nindexed_path: {}",
        working_directory.display(),
        resolved.rerouted,
        resolved.target_path.display(),
        resolved.indexed_path.display(),
    )
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

const WRITE_SEMANTICS_LINE_PREFIX: &str = "Write semantics: ";

/// Parse the normative `Write semantics:` envelope line from legacy edit tool output.
pub(crate) fn parse_write_semantics_from_output(output: &str) -> Option<EditWriteSemantics> {
    output.lines().find_map(|line| {
        let label = line.strip_prefix(WRITE_SEMANTICS_LINE_PREFIX)?.trim();
        Some(match label {
            "dry run (no writes)" => EditWriteSemantics::DryRunNoWrites,
            "atomic write + reindex" => EditWriteSemantics::AtomicWriteAndReindex,
            "transactional write + rollback + reindex" => {
                EditWriteSemantics::TransactionalWriteRollbackAndReindex
            }
            _ => return None,
        })
    })
}

fn edit_output_is_error(output: &str) -> bool {
    output.starts_with("Error:")
        || output.starts_with("Invalid")
        || output.contains(": edit safety blocked")
        || output.starts_with("Index not loaded.")
        || output.starts_with("Index is loading")
        || output.starts_with("Index degraded:")
        || output.starts_with("File not found:")
        || output.starts_with("Symbol not found:")
}

/// Whether legacy edit output indicates a committed write (not dry-run preview).
pub(crate) fn edit_output_bytes_committed(output: &str) -> bool {
    if edit_output_is_error(output) {
        return false;
    }
    matches!(
        parse_write_semantics_from_output(output),
        Some(EditWriteSemantics::AtomicWriteAndReindex)
            | Some(EditWriteSemantics::TransactionalWriteRollbackAndReindex)
    )
}

/// Write mode for compact `symforge_edit` apply metadata (`committed` / `dry_run` / `failed`).
pub(crate) fn symforge_edit_apply_write_mode(output: &str) -> &'static str {
    if edit_output_is_error(output) {
        return "failed";
    }
    match parse_write_semantics_from_output(output) {
        Some(EditWriteSemantics::DryRunNoWrites) => "dry_run",
        Some(EditWriteSemantics::AtomicWriteAndReindex)
        | Some(EditWriteSemantics::TransactionalWriteRollbackAndReindex) => "committed",
        None => "failed",
    }
}

/// Whether a `symforge_edit` body reports an internal failure (as opposed to an
/// invalid request, classified separately by the caller).
///
/// `apply` gates the apply-only failure sentinels (`Write mode: failed` from
/// `stel::edit_apply::format_apply_metadata`, and `: edit safety blocked` from
/// [`format_capability_warning`]) so a dry-run preview is never misclassified
/// as a failed write. `Index not loaded.` is unconditional because the index is
/// unavailable for previews and applies alike. `tool_body` is the trust/result
/// envelope text; `full_body` is the complete summary carrying apply metadata.
pub(crate) fn symforge_edit_internal_failure(
    tool_body: &str,
    apply: bool,
    full_body: &str,
) -> bool {
    if tool_body.starts_with("Index not loaded.") {
        return true;
    }
    apply
        && (full_body.contains("Write mode: failed") || tool_body.contains(": edit safety blocked"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_edit_envelope() {
        let result = format_edit_envelope(
            EditSafetyMode::StructuralEditSafe,
            EditSourceAuthority::DiskRefreshed,
            EditWriteSemantics::AtomicWriteAndReindex,
            "src/lib.rs:12",
        );
        assert!(result.contains("Edit safety: structural-edit-safe"));
        assert!(result.contains("Path authority: repository-bound"));
        assert!(result.contains("Source authority: disk-refreshed"));
        assert!(result.contains("Write semantics: atomic write + reindex"));
        assert!(result.contains("src/lib.rs:12"));
    }

    #[test]
    fn test_format_capability_warning() {
        let result = format_capability_warning(
            "replace_symbol_body",
            "html",
            "structural-edit-safe",
            "text-edit-safe",
            "use edit_within_symbol",
        );
        assert!(result.contains("edit safety blocked"));
        assert!(result.contains("Required safety: structural-edit-safe"));
        assert!(result.contains("Available safety: text-edit-safe"));
        assert!(result.contains("Language: html"));
    }

    #[test]
    fn edit_output_bytes_committed_uses_write_semantics_not_summary_wording() {
        let envelope = format_edit_envelope(
            EditSafetyMode::StructuralEditSafe,
            EditSourceAuthority::DiskRefreshed,
            EditWriteSemantics::AtomicWriteAndReindex,
            "src/lib.rs:1",
        );
        let committed = format!("{envelope}\nsrc/lib.rs — updated function `foo` (10 → 12 bytes)");
        assert!(edit_output_bytes_committed(&committed));
        assert_eq!(symforge_edit_apply_write_mode(&committed), "committed");

        let dry = format!(
            "{}\n[DRY RUN] Would replace `foo` in src/lib.rs",
            format_edit_envelope(
                EditSafetyMode::StructuralEditSafe,
                EditSourceAuthority::DiskRefreshed,
                EditWriteSemantics::DryRunNoWrites,
                "src/lib.rs:1",
            )
        );
        assert!(!edit_output_bytes_committed(&dry));
        assert_eq!(symforge_edit_apply_write_mode(&dry), "dry_run");

        let blocked = format_capability_warning(
            "replace_symbol_body",
            "html",
            "structural-edit-safe",
            "text-edit-safe",
            "use edit_within_symbol",
        );
        assert!(!edit_output_bytes_committed(&blocked));
        assert_eq!(symforge_edit_apply_write_mode(&blocked), "failed");
    }

    #[test]
    fn symforge_edit_internal_failure_gates_apply_only_sentinels() {
        // Index-unavailable is unconditional: a failure for preview and apply alike.
        assert!(symforge_edit_internal_failure(
            "Index not loaded.",
            false,
            "Index not loaded."
        ));
        assert!(symforge_edit_internal_failure(
            "Index not loaded.",
            true,
            "Index not loaded."
        ));

        // `Write mode: failed` only counts as a failure on an apply, never a preview.
        let failed_apply = "Write mode: failed\nChanged file: src/a.rs";
        assert!(symforge_edit_internal_failure("ok", true, failed_apply));
        assert!(!symforge_edit_internal_failure("ok", false, failed_apply));

        // `: edit safety blocked` is likewise apply-gated.
        let blocked = format_capability_warning(
            "replace_symbol_body",
            "html",
            "structural-edit-safe",
            "text-edit-safe",
            "use edit_within_symbol",
        );
        assert!(symforge_edit_internal_failure(&blocked, true, &blocked));
        assert!(!symforge_edit_internal_failure(&blocked, false, &blocked));

        // A clean committed apply is not an internal failure.
        let committed = format!(
            "{}\nWrite mode: committed",
            format_edit_envelope(
                EditSafetyMode::StructuralEditSafe,
                EditSourceAuthority::DiskRefreshed,
                EditWriteSemantics::AtomicWriteAndReindex,
                "src/a.rs:1",
            )
        );
        assert!(!symforge_edit_internal_failure(
            &committed, true, &committed
        ));
    }

    #[test]
    fn parse_write_semantics_from_output_reads_envelope_line() {
        let output = format_edit_envelope(
            EditSafetyMode::StructuralEditSafe,
            EditSourceAuthority::CurrentIndex,
            EditWriteSemantics::TransactionalWriteRollbackAndReindex,
            "src/a.rs:3",
        );
        assert_eq!(
            parse_write_semantics_from_output(&output),
            Some(EditWriteSemantics::TransactionalWriteRollbackAndReindex)
        );
    }

    #[test]
    fn test_format_batch_envelope() {
        let result = format_batch_envelope(
            EditSafetyMode::StructuralEditSafe,
            MatchType::Constrained,
            EditSourceAuthority::CurrentIndex,
            EditWriteSemantics::TransactionalWriteRollbackAndReindex,
            "definition `src/lib.rs` + 2 target file(s)",
        );
        assert!(result.contains("Edit safety: structural-edit-safe"));
        assert!(result.contains("Match type: constrained"));
        assert!(result.contains("Source authority: current index"));
        assert!(result.contains("Write semantics: transactional write + rollback + reindex"));
        assert!(result.contains("definition `src/lib.rs` + 2 target file(s)"));
    }

    #[test]
    fn test_format_replace() {
        let result = format_replace("src/lib.rs", "process", "fn", 342, 287);
        assert!(result.contains("src/lib.rs"));
        assert!(result.contains("process"));
        assert!(result.contains("342"));
        assert!(result.contains("287"));
    }

    #[test]
    fn test_format_insert() {
        let result = format_insert("src/lib.rs", "handler", "after", 120);
        assert!(result.contains("src/lib.rs"));
        assert!(result.contains("after"));
        assert!(result.contains("handler"));
        assert!(result.contains("120"));
    }

    #[test]
    fn test_format_delete() {
        let result = format_delete("src/lib.rs", "old_fn", "fn", 200);
        assert!(result.contains("src/lib.rs"));
        assert!(result.contains("old_fn"));
        assert!(result.contains("200"));
    }

    #[test]
    fn test_format_edit_within() {
        let result = format_edit_within("src/lib.rs", "process", 2, 500, 480);
        assert!(result.contains("src/lib.rs"));
        assert!(result.contains("process"));
        assert!(result.contains("2"));
    }

    #[test]
    fn test_format_stale_warnings_empty() {
        let result = format_stale_warnings("src/lib.rs", "foo", &[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_format_stale_warnings_with_refs() {
        let refs = vec![
            ("src/main.rs".to_string(), 45, Some("fn main".to_string())),
            ("src/handler.rs".to_string(), 23, None),
        ];
        let result = format_stale_warnings("src/lib.rs", "process", &refs);
        assert!(result.contains("src/main.rs:45"));
        assert!(result.contains("fn main"));
        assert!(result.contains("src/handler.rs:23"));
        assert!(result.contains("2 reference(s)"));
    }

    #[test]
    fn test_format_batch_summary() {
        let results = vec![
            "src/a.rs — replaced `foo`".to_string(),
            "src/b.rs — deleted `bar`".to_string(),
        ];
        let result = format_batch_summary(&results, 2);
        assert!(result.contains("2 edit(s)"));
        assert!(result.contains("2 file(s)"));
    }
}
