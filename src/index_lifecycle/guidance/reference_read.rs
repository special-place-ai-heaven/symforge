//! Shared reference/dependent evidence, bounded rendering, and qualification.
use super::reference_contract::FindReferencesInput;
use crate::domain::ReferenceKind;
use crate::live_index::qualified_usages::QualifiedUsage;
use crate::live_index::{
    FindDependentsView, FindReferencesView, ImplementationsView, LiveIndex,
    ReferenceContextLineView, ReferenceFileView, ReferenceHitView,
};
use std::collections::HashSet;

/// Budget limits for reference/dependent output to prevent unbounded token usage.
pub struct OutputLimits {
    /// Maximum number of files to include in the output.
    pub max_files: usize,
    /// Maximum number of reference/hit lines per file.
    pub max_per_file: usize,
    /// Maximum total hits across all files (max_files * max_per_file).
    pub total_hits: usize,
}

impl OutputLimits {
    pub fn new(max_files: u32, max_per_file: u32) -> Self {
        Self {
            max_files: max_files.min(100) as usize,
            max_per_file: max_per_file.min(50) as usize,
            total_hits: (max_files.min(100) * max_per_file.min(50)) as usize,
        }
    }
}

impl Default for OutputLimits {
    fn default() -> Self {
        Self {
            max_files: 20,
            max_per_file: 10,
            total_hits: 200,
        }
    }
}

/// Find all references for a name across the repo, grouped by file with 3-line context.
///
/// kind_filter: "call" | "import" | "type_usage" | "all" | None (all)
/// Output format matches CONTEXT.md decision AD-6 (compact human-readable).
#[cfg(feature = "server")]
pub fn find_references_result(index: &LiveIndex, name: &str, kind_filter: Option<&str>) -> String {
    let limits = OutputLimits::default();
    let view = index.capture_find_references_view(name, kind_filter, limits.total_hits);
    find_references_result_view(&view, name, &limits)
}

pub fn find_references_result_view(
    view: &FindReferencesView,
    name: &str,
    limits: &OutputLimits,
) -> String {
    if view.total_refs == 0 {
        let mut lines = vec![format!(
            "No references found for \"{name}\". Either the name is misspelled (try search_symbols(query={name})), it has no callers (public API, entry point, or dead code), or you wanted implementors (try find_references with mode=implementations)."
        )];
        append_reference_target_candidates(&mut lines, view, true);
        return lines.join("\n");
    }

    let total = view.total_refs;
    let total_files = view.total_files;
    let shown_files = view.files.len().min(limits.max_files);
    let mut lines = if shown_files < total_files {
        vec![format!(
            "{total} references across {total_files} files (showing {shown_files})  [1.00]"
        )]
    } else {
        vec![format!("{total} references in {total_files} files  [1.00]")]
    };
    if view.total_refs > 50 {
        lines.push(format!(
            "Note: '{}' is a very common identifier — results may include unrelated symbols. \
             Add path or symbol_kind to scope the search.",
            name
        ));
    }
    lines.push(String::new()); // blank line
    append_reference_target_candidates(&mut lines, view, true);
    if view
        .files
        .iter()
        .any(|file| file.caller_declaration_count > 0 || file.caller_header_unavailable_count > 0)
    {
        lines.push(
            "Caller declaration line numbers are 1-based; definition ranges are inclusive."
                .to_string(),
        );
    }

    let mut total_emitted = 0usize;
    for file in view.files.iter().take(limits.max_files) {
        if total_emitted >= limits.total_hits {
            break;
        }
        lines.push(file.file_path.clone());
        append_caller_declarations(&mut lines, file, true);
        let mut hit_count = 0usize;
        let mut truncated_hits = 0usize;
        for hit in &file.hits {
            if hit_count >= limits.max_per_file || total_emitted >= limits.total_hits {
                truncated_hits += 1;
                continue;
            }
            for line in &hit.context_lines {
                if line.is_reference_line {
                    if let Some(annotation) = &line.enclosing_annotation {
                        lines.push(format!(
                            "  {}: {:<40}{}",
                            line.line_number, line.text, annotation
                        ));
                    } else {
                        lines.push(format!("  {}: {}", line.line_number, line.text));
                    }
                } else {
                    lines.push(format!("  {}: {}", line.line_number, line.text));
                }
            }
            hit_count += 1;
            total_emitted += 1;
        }
        if truncated_hits > 0 {
            lines.push(format!("  ... and {truncated_hits} more references"));
        }
        lines.push(String::new()); // blank line between files
    }

    let remaining_files = total_files.saturating_sub(shown_files);
    if remaining_files > 0 {
        lines.push(format!("... and {remaining_files} more files"));
    }

    while lines.last().map(|l| l.is_empty()).unwrap_or(false) {
        lines.pop();
    }

    lines.join("\n")
}

fn append_reference_target_candidates(
    lines: &mut Vec<String>,
    view: &FindReferencesView,
    include_header_text: bool,
) {
    if view.target_candidate_count == 0 {
        lines.push(format!(
            "No exact-name definitions currently indexed; indexed-file parse coverage {}.",
            if view.target_indexed_file_parse_coverage_complete {
                "complete"
            } else {
                "incomplete"
            },
        ));
        return;
    }
    lines.push(format!(
        "Target definition candidates: {} exact-name indexed symbols; indexed-file parse coverage {}. Unique among indexed records: {} (compiler resolution not available).",
        view.target_candidate_count,
        if view.target_indexed_file_parse_coverage_complete {
            "complete"
        } else {
            "incomplete"
        },
        if view.target_indexed_file_parse_coverage_complete && view.target_candidate_count == 1 {
            "yes"
        } else {
            "no / unproven"
        },
    ));
    for candidate in &view.target_candidates {
        lines.push(format!(
            "  {}:L{} [{}] bytes {}..{}",
            candidate.path,
            candidate.line_range.0.saturating_add(1),
            candidate.kind,
            candidate.byte_range.0,
            candidate.byte_range.1,
        ));
        if include_header_text && let Some(header) = &candidate.header {
            lines.extend(header.lines().map(|line| format!("    {line}")));
        } else if include_header_text {
            lines.push("    declaration header unavailable or ambiguous".to_string());
        }
    }
    let omitted = view
        .target_candidate_count
        .saturating_sub(view.target_candidates.len());
    if omitted > 0 {
        lines.push(format!(
            "  ... {omitted} more candidates omitted by the display cap"
        ));
    }
}

fn append_caller_declarations(
    lines: &mut Vec<String>,
    file: &crate::live_index::ReferenceFileView,
    include_header_text: bool,
) {
    if file.caller_declaration_count == 0 && file.caller_header_unavailable_count == 0 {
        return;
    }
    lines.push(format!(
        "  AST declaration headers for returned sites: {} distinct (showing {}; {} omitted by metadata cap; {} caller headers unavailable; final response token budget may truncate):",
        file.caller_declaration_count,
        file.caller_declarations.len(),
        file.caller_declarations_omitted,
        file.caller_header_unavailable_count,
    ));
    for declaration in &file.caller_declarations {
        let signature_start = match (declaration.is_callable, declaration.signature_start_line) {
            (true, Some(line)) => format!("; signature start L{line}"),
            (true, None) => "; signature start unavailable".to_string(),
            (false, _) => String::new(),
        };
        lines.push(format!(
            "    {}: definition L{}-{} bytes {}..{}; [{}] header L{} bytes {}..{}{}",
            declaration.scope,
            declaration.definition_line_range.0,
            declaration.definition_line_range.1,
            declaration.definition_byte_range.0,
            declaration.definition_byte_range.1,
            declaration.node_kind,
            declaration.line_number,
            declaration.byte_range.0,
            declaration.byte_range.1,
            signature_start,
        ));
        if include_header_text {
            lines.extend(declaration.text.lines().map(|line| format!("      {line}")));
        }
        if declaration.scope == "caller" && declaration.is_callable {
            // Only the verified AST name node is eligible as a caller identity.
            match &declaration.identity {
                Some(identity) if include_header_text => {
                    lines.push(format!("      declared caller: {}", identity.name));
                }
                Some(identity) => lines.push(format!(
                    "      caller identity: {} bytes {}..{}",
                    identity.name, identity.byte_range.0, identity.byte_range.1
                )),
                None if include_header_text => {}
                None => lines.push("      caller identity unavailable".to_string()),
            }
        }
    }
}

/// Render a compact find_references result: file:line \[kind\] in symbol — no source text.
pub fn find_references_compact_view(
    view: &FindReferencesView,
    name: &str,
    limits: &OutputLimits,
) -> String {
    if view.total_refs == 0 {
        let mut lines = vec![format!(
            "No references found for \"{name}\". Either the name is misspelled (try search_symbols(query={name})), it has no callers (public API, entry point, or dead code), or you wanted implementors (try find_references with mode=implementations)."
        )];
        append_reference_target_candidates(&mut lines, view, false);
        return lines.join("\n");
    }

    let total_files = view.total_files;
    let shown_files = view.files.len().min(limits.max_files);
    let mut lines = if shown_files < total_files {
        vec![format!(
            "{} references to \"{}\" across {} files (showing {})",
            view.total_refs, name, total_files, shown_files
        )]
    } else {
        vec![format!(
            "{} references to \"{}\" in {} files",
            view.total_refs, name, total_files
        )]
    };
    append_reference_target_candidates(&mut lines, view, false);
    if view.total_refs > 50 {
        lines.push(format!(
            "Note: '{}' is a very common identifier — results may include unrelated symbols. \
             Add path or symbol_kind to scope the search.",
            name
        ));
    }

    let mut total_emitted = 0usize;
    for file in view.files.iter().take(limits.max_files) {
        if total_emitted >= limits.total_hits {
            break;
        }
        lines.push(file.file_path.clone());
        append_caller_declarations(&mut lines, file, false);
        let mut hit_count = 0usize;
        let mut truncated_hits = 0usize;
        for hit in &file.hits {
            if hit_count >= limits.max_per_file || total_emitted >= limits.total_hits {
                truncated_hits += 1;
                continue;
            }
            for line in &hit.context_lines {
                if line.is_reference_line {
                    let annotation = line.enclosing_annotation.as_deref().unwrap_or("");
                    lines.push(format!("  :{} {}", line.line_number, annotation));
                }
            }
            hit_count += 1;
            total_emitted += 1;
        }
        if truncated_hits > 0 {
            lines.push(format!("  ... and {truncated_hits} more"));
        }
    }

    let remaining_files = total_files.saturating_sub(shown_files);
    if remaining_files > 0 {
        lines.push(format!("... and {remaining_files} more files"));
    }

    lines.join("\n")
}

/// Format results of `find_references` implementations mode.
pub fn implementations_result_view(
    view: &ImplementationsView,
    name: &str,
    limits: &OutputLimits,
) -> String {
    if view.entries.is_empty() {
        return format!("No implementations found for \"{name}\"");
    }

    let total = view.entries.len();
    let shown = total.min(limits.max_files * limits.max_per_file);
    let mut lines = vec![format!("{total} implementation(s) found for \"{name}\"")];
    lines.push(String::new());

    // Group by trait name for readable output
    let mut current_trait: Option<&str> = None;
    for (i, entry) in view.entries.iter().enumerate() {
        if i >= shown {
            break;
        }
        if current_trait != Some(&entry.trait_name) {
            if current_trait.is_some() {
                lines.push(String::new());
            }
            lines.push(format!("trait/interface {}:", entry.trait_name));
            current_trait = Some(&entry.trait_name);
        }
        lines.push(format!(
            "  {} ({}:{})",
            entry.implementor,
            entry.file_path,
            entry.line + 1
        ));
    }

    let remaining = total.saturating_sub(shown);
    if remaining > 0 {
        lines.push(String::new());
        lines.push(format!("... and {remaining} more"));
    }

    lines.join("\n")
}

/// Find all files that import (depend on) the given path.
///
/// Output format: compact list grouped by importing file, each with import line.
#[cfg(feature = "server")]
pub fn find_dependents_result(index: &LiveIndex, path: &str) -> String {
    let view = index.capture_find_dependents_view(path);
    find_dependents_result_view(&view, path, &OutputLimits::default())
}

pub fn find_dependents_result_view(
    view: &FindDependentsView,
    path: &str,
    limits: &OutputLimits,
) -> String {
    if view.files.is_empty() {
        return format!(
            "No file-level dependents found for \"{path}\"\nTip: use find_references(name=\"<symbol>\", path=\"{path}\") for symbol-level callers/usages."
        );
    }

    let total_files = view.files.len();
    let shown_files = total_files.min(limits.max_files);
    let mut lines = vec![
        format!("File-level dependency graph: {total_files} files depend on {path}"),
        "Need symbol-level callers/usages instead? Use find_references(name=\"<symbol>\", path=\"<file>\")."
            .to_string(),
    ];
    lines.push(String::new()); // blank line

    for file in view.files.iter().take(limits.max_files) {
        lines.push(file.file_path.clone());
        let total_refs = file.lines.len();
        let shown_refs = total_refs.min(limits.max_per_file);
        for line in file.lines.iter().take(limits.max_per_file) {
            lines.push(format!(
                "  {}: {}   [{}]",
                line.line_number, line.line_content, line.kind
            ));
        }
        let remaining_refs = total_refs.saturating_sub(shown_refs);
        if remaining_refs > 0 {
            lines.push(format!("  ... and {remaining_refs} more references"));
        }
        lines.push(String::new()); // blank line between files
    }

    let remaining_files = total_files.saturating_sub(shown_files);
    if remaining_files > 0 {
        lines.push(format!("... and {remaining_files} more files"));
    }

    // Remove trailing blank line
    while lines.last().map(|l| l.is_empty()).unwrap_or(false) {
        lines.pop();
    }

    lines.join("\n")
}

/// Render a compact find_dependents result: file:line \[kind\] without source text.
pub fn find_dependents_compact_view(
    view: &FindDependentsView,
    path: &str,
    limits: &OutputLimits,
) -> String {
    if view.files.is_empty() {
        return format!(
            "No file-level dependents found for \"{path}\"\nTip: use find_references(name=\"<symbol>\", path=\"{path}\") for symbol-level callers/usages."
        );
    }

    let total_files = view.files.len();
    let shown_files = total_files.min(limits.max_files);
    let mut lines = vec![
        format!("File-level dependency graph: {total_files} files depend on {path}"),
        "Need symbol-level callers/usages instead? Use find_references(name=\"<symbol>\", path=\"<file>\")."
            .to_string(),
    ];

    for file in view.files.iter().take(limits.max_files) {
        let total_refs = file.lines.len();
        // Count references by kind across ALL lines for this file (not just the
        // first max_per_file), so the per-file summary reflects the true totals.
        let mut kind_counts: std::collections::BTreeMap<&str, usize> =
            std::collections::BTreeMap::new();
        for line in &file.lines {
            *kind_counts.entry(line.kind.as_str()).or_insert(0) += 1;
        }
        let summary = if kind_counts.is_empty() {
            format!("  {}  ({total_refs} refs)", file.file_path)
        } else {
            let breakdown: Vec<String> = kind_counts
                .iter()
                .map(|(kind, count)| format!("{count} {kind}"))
                .collect();
            format!(
                "  {}  ({} refs: {})",
                file.file_path,
                total_refs,
                breakdown.join(", ")
            )
        };
        lines.push(summary);
    }

    let remaining_files = total_files.saturating_sub(shown_files);
    if remaining_files > 0 {
        lines.push(format!("... and {remaining_files} more files"));
    }

    lines.join("\n")
}

/// Render a find_dependents result as a Mermaid flowchart.
pub fn find_dependents_mermaid(
    view: &FindDependentsView,
    path: &str,
    limits: &OutputLimits,
) -> String {
    if view.files.is_empty() {
        return format!("No dependents found for \"{path}\"");
    }

    let mut lines = vec!["flowchart LR".to_string()];
    let target_id = mermaid_node_id(path);
    lines.push(format!("    {target_id}[\"{path}\"]"));

    for file in view.files.iter().take(limits.max_files) {
        let dep_id = mermaid_node_id(&file.file_path);
        let ref_count = file.lines.len();

        let mut names: Vec<&str> = Vec::new();
        for line in &file.lines {
            if !names.contains(&line.name.as_str()) {
                names.push(&line.name);
                if names.len() >= 3 {
                    break;
                }
            }
        }
        let remaining = ref_count.saturating_sub(names.len());
        let label = if names.is_empty() {
            format!("{ref_count} refs")
        } else if remaining > 0 {
            format!("{} +{remaining}", names.join(", "))
        } else {
            names.join(", ")
        };
        lines.push(format!(
            "    {dep_id}[\"{}\"] -->|\"{label}\"| {target_id}",
            file.file_path
        ));
    }

    let remaining = view.files.len().saturating_sub(limits.max_files);
    if remaining > 0 {
        lines.push(format!(
            "    more[\"... and {remaining} more files\"] --> {target_id}"
        ));
    }

    lines.join("\n")
}

/// Render a find_dependents result as a Graphviz DOT digraph.
pub fn find_dependents_dot(view: &FindDependentsView, path: &str, limits: &OutputLimits) -> String {
    if view.files.is_empty() {
        return format!("No dependents found for \"{path}\"");
    }

    let mut lines = vec!["digraph dependents {".to_string()];
    lines.push("    rankdir=LR;".to_string());
    lines.push(format!(
        "    \"{}\" [shape=box, style=bold];",
        dot_escape(path)
    ));

    for file in view.files.iter().take(limits.max_files) {
        let mut names: Vec<&str> = Vec::new();
        for line in &file.lines {
            if !names.contains(&line.name.as_str()) {
                names.push(&line.name);
                if names.len() >= 3 {
                    break;
                }
            }
        }
        let remaining = file.lines.len().saturating_sub(names.len());
        let label = if names.is_empty() {
            format!("{} refs", file.lines.len())
        } else if remaining > 0 {
            format!("{} +{remaining}", names.join(", "))
        } else {
            names.join(", ")
        };
        lines.push(format!(
            "    \"{}\" -> \"{}\" [label=\"{}\"];",
            dot_escape(&file.file_path),
            dot_escape(path),
            label
        ));
    }

    let remaining = view.files.len().saturating_sub(limits.max_files);
    if remaining > 0 {
        lines.push(format!(
            "    \"... and {} more\" -> \"{}\" [style=dashed];",
            remaining,
            dot_escape(path)
        ));
    }

    lines.push("}".to_string());
    lines.join("\n")
}

/// Sanitize a file path into a valid Mermaid node ID (alphanumeric + underscores).
fn mermaid_node_id(path: &str) -> String {
    path.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect()
}

/// Escape a string for DOT label/node usage.
fn dot_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

pub(crate) fn find_references_match_type_label(
    input: &FindReferencesInput,
    mode: &str,
) -> &'static str {
    if mode == "implementations" {
        return "constrained (implementations mode)";
    }
    if input.path.is_some() && input.symbol_line.is_some() {
        "exact"
    } else if input.path.is_some() {
        "constrained (path-scoped symbol)"
    } else if input.symbol_kind.is_some() || input.kind.as_deref().is_some_and(|kind| kind != "all")
    {
        "constrained (repo-wide filtered symbol)"
    } else {
        "constrained (repo-wide name match)"
    }
}

pub(crate) fn find_references_scope_summary(input: &FindReferencesInput, mode: &str) -> String {
    let mut parts = Vec::new();
    if mode == "implementations" {
        parts.push(format!(
            "repo-wide implementations for symbol token `{}`",
            input.name
        ));
        parts.push(format!(
            "direction `{}`",
            input.direction.as_deref().unwrap_or("auto")
        ));
        return parts.join("; ");
    }

    match input.path.as_deref() {
        Some(path) => parts.push(format!("path `{path}`")),
        None => parts.push(format!("repo-wide symbol token `{}`", input.name)),
    }
    if let Some(line) = input.symbol_line {
        parts.push(format!("exact selector line {line}"));
    }
    if let Some(symbol_kind) = input.symbol_kind.as_deref() {
        parts.push(format!("symbol kind `{symbol_kind}`"));
    }
    if let Some(reference_kind) = input.kind.as_deref().filter(|kind| *kind != "all") {
        parts.push(format!("reference kind `{reference_kind}`"));
    }
    parts.join("; ")
}

pub(crate) fn find_references_kind_is_best_effort(kind: Option<&str>) -> bool {
    // Mirror `find_references_kind_filter` rather than duplicating a string
    // whitelist, so the caveat can never drift from the kinds actually returned.
    // A filter result of `None` means "no kind filter" -> all kinds returned,
    // INCLUDING best-effort type/value usages; that covers `all`/`None` AND any
    // UNRECOGNIZED kind (the filter's `_ => None` arm), so a typo'd `kind` still
    // gets the caveat instead of silently shipping a best-effort trace unmarked.
    matches!(
        find_references_kind_filter(kind),
        None | Some(ReferenceKind::TypeUsage) | Some(ReferenceKind::ValueUse)
    )
}

pub(crate) fn find_references_completeness_label(
    view: &crate::live_index::FindReferencesView,
    limits: &OutputLimits,
    kind: Option<&str>,
) -> String {
    let shown_files = view.files.len().min(limits.max_files);
    let mut shown_refs = 0usize;
    for file in view.files.iter().take(limits.max_files) {
        if shown_refs >= limits.total_hits {
            break;
        }
        let remaining_budget = limits.total_hits.saturating_sub(shown_refs);
        shown_refs += file
            .hits
            .len()
            .min(limits.max_per_file)
            .min(remaining_budget);
    }
    let omitted_refs = view.total_refs.saturating_sub(shown_refs);
    let omitted_files = view.total_files.saturating_sub(shown_files);
    // Recall-confidence caveat, targeted (not blanket): only type/value-usage
    // traces are best-effort, so only they carry it. Riding the existing
    // completeness label keeps it one site, one envelope, no ResultStatus bump.
    let recall_caveat = if find_references_kind_is_best_effort(kind) {
        " — usage-trace recall is best-effort; dynamic dispatch, macro-generated, \
         reflective, and cross-language usages may be missed"
    } else {
        ""
    };
    if omitted_refs == 0 && omitted_files == 0 {
        return format!("full for current scope{recall_caveat}");
    }
    let mut parts = vec!["truncated by result cap".to_string()];
    if omitted_refs > 0 {
        parts.push(format!("{omitted_refs} reference(s) omitted"));
    }
    if omitted_files > 0 {
        parts.push(format!("{omitted_files} file(s) omitted"));
    }
    format!("{}{recall_caveat}", parts.join("; "))
}

pub(crate) fn find_references_evidence(view: &crate::live_index::FindReferencesView) -> String {
    let anchors = view
        .files
        .iter()
        .flat_map(|file| {
            let file_path = file.file_path.clone();
            file.hits.iter().flat_map(move |hit| {
                hit.context_lines
                    .iter()
                    .filter(|line| line.is_reference_line)
                    .map({
                        let file_path = file_path.clone();
                        move |line| format!("{file_path}:{}", line.line_number)
                    })
            })
        })
        .take(3)
        .collect();
    anchored_search_evidence(anchors, "reference anchors")
}

pub(crate) fn find_references_kind_filter(kind_filter: Option<&str>) -> Option<ReferenceKind> {
    match kind_filter {
        Some("call") => Some(ReferenceKind::Call),
        Some("import") => Some(ReferenceKind::Import),
        Some("type_usage") => Some(ReferenceKind::TypeUsage),
        Some("macro_use") => Some(ReferenceKind::MacroUse),
        Some("value_use") => Some(ReferenceKind::ValueUse),
        Some("all") | None => None,
        _ => None,
    }
}

pub(crate) fn should_collect_qualified_usages(input: &FindReferencesInput) -> bool {
    input.path.is_none()
        && !input.name.is_empty()
        && matches!(
            input.kind.as_deref(),
            None | Some("all") | Some("type_usage")
        )
}

pub(crate) fn qualified_usage_hit_view(usage: &QualifiedUsage) -> ReferenceHitView {
    let confidence = if usage.confident {
        "confident"
    } else {
        "uncertain"
    };
    let annotation = format!("[qualified-path scan: {confidence}]");

    ReferenceHitView {
        context_lines: vec![ReferenceContextLineView {
            line_number: usage.line,
            text: usage.line_text.clone(),
            is_reference_line: true,
            enclosing_annotation: Some(annotation),
        }],
    }
}

pub(crate) fn merge_qualified_usages_into_view(
    view: &mut FindReferencesView,
    mut usages: Vec<QualifiedUsage>,
    mut seen_ranges: HashSet<(String, (u32, u32))>,
) {
    let mut known_files: HashSet<String> = seen_ranges
        .iter()
        .map(|(file_path, _)| file_path.clone())
        .collect();
    known_files.extend(view.files.iter().map(|file| file.file_path.clone()));

    usages.sort_by(|a, b| {
        a.file_path
            .cmp(&b.file_path)
            .then(a.line.cmp(&b.line))
            .then(a.byte_range.0.cmp(&b.byte_range.0))
    });

    let mut added = 0usize;
    for usage in usages {
        let range_key = (usage.file_path.clone(), usage.byte_range);
        if !seen_ranges.insert(range_key) {
            continue;
        }
        let hit = qualified_usage_hit_view(&usage);
        if let Some(file_view) = view
            .files
            .iter_mut()
            .find(|file| file.file_path == usage.file_path)
        {
            file_view.hits.push(hit);
        } else {
            view.files.push(ReferenceFileView {
                file_path: usage.file_path.clone(),
                hits: vec![hit],
                caller_declarations: Vec::new(),
                caller_declaration_count: 0,
                caller_declarations_omitted: 0,
                caller_header_unavailable_count: 0,
            });
        }
        known_files.insert(usage.file_path);
        added += 1;
    }

    if added > 0 {
        view.total_refs += added;
        view.total_files = known_files.len();
        view.files.sort_by(|a, b| a.file_path.cmp(&b.file_path));
    }
}

pub(crate) fn implementations_parse_state_for_paths(
    index: &LiveIndex,
    view: &crate::live_index::ImplementationsView,
) -> &'static str {
    search_parse_state_for_paths(
        index,
        view.entries.iter().map(|entry| entry.file_path.as_str()),
    )
}

pub(crate) fn implementations_completeness_label(
    view: &crate::live_index::ImplementationsView,
    limits: &OutputLimits,
) -> String {
    let shown = view
        .entries
        .len()
        .min(limits.max_files * limits.max_per_file);
    let omitted = view.entries.len().saturating_sub(shown);
    if omitted == 0 {
        "full for current scope".to_string()
    } else {
        format!("truncated by result cap ({omitted} implementation entry(s) omitted)")
    }
}

pub(crate) fn implementations_evidence(view: &crate::live_index::ImplementationsView) -> String {
    let anchors = view
        .entries
        .iter()
        .take(3)
        .map(|entry| format!("{}:{}", entry.file_path, entry.line + 1))
        .collect();
    anchored_search_evidence(anchors, "implementation anchors")
}

pub(crate) fn search_parse_state_for_paths<'a, I>(index: &LiveIndex, paths: I) -> &'static str
where
    I: IntoIterator<Item = &'a str>,
{
    if paths
        .into_iter()
        .filter_map(|path| index.get_file(path))
        .any(|file| file.parse_diagnostic.is_some())
    {
        "partial"
    } else {
        "parsed"
    }
}

pub(crate) fn anchored_search_evidence(anchors: Vec<String>, noun: &str) -> String {
    if anchors.is_empty() {
        format!("no {noun} available")
    } else {
        let rendered = anchors
            .into_iter()
            .map(|anchor| format!("`{anchor}`"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("{noun} {rendered}")
    }
}

pub(crate) fn symbol_candidate_paths(
    index: &crate::live_index::store::LiveIndex,
    name: &str,
) -> Vec<String> {
    let mut candidates: Vec<String> = index
        .all_files()
        .filter_map(|(path, file)| {
            if file.symbols.iter().any(|s| s.name == name) {
                Some(path.to_string())
            } else {
                None
            }
        })
        .collect();
    candidates.sort();
    candidates.dedup();
    candidates
}

pub(crate) fn tier2_reference_disclosure_with(
    candidates: &[(String, u64)],
    name: &str,
    has_root: bool,
    mut read: impl FnMut(&str, usize) -> Option<Vec<u8>>,
) -> Option<String> {
    if candidates.is_empty() || name.trim().is_empty() {
        return None;
    }
    // ponytail: bounded whole-file reads + str::contains; the OS page cache
    // makes repeated sweeps cheap. Budgets keep the pathological repo honest
    // (reported as unswept) instead of slow.
    const MAX_SWEEP_FILES: usize = 8;
    const MAX_SWEEP_BYTES: u64 = 32 * 1024 * 1024;
    let preview = |paths: &[&str]| -> String {
        let shown: Vec<&str> = paths.iter().take(3).copied().collect();
        let suffix = if paths.len() > shown.len() {
            format!(", … (+{} more)", paths.len() - shown.len())
        } else {
            String::new()
        };
        format!("{}{}", shown.join(", "), suffix)
    };
    if !has_root {
        // No resolvable root: cannot sweep, but silence would be the lie.
        let paths: Vec<&str> = candidates.iter().map(|(path, _)| path.as_str()).collect();
        return Some(format!(
            "Tier-2 exclusion: {} first-party file(s) over the size threshold (1MB data / 4MB code) were NOT reference-scanned \
             (metadata-only) and could not be swept for \"{name}\" (no repo root): {}",
            candidates.len(),
            preview(&paths),
        ));
    };
    let mut matched: Vec<&str> = Vec::new();
    let mut unswept: Vec<&str> = Vec::new();
    let mut bytes_budget = MAX_SWEEP_BYTES;
    for (i, (path, size)) in candidates.iter().enumerate() {
        if i >= MAX_SWEEP_FILES || bytes_budget < *size {
            unswept.push(path.as_str());
            continue;
        }
        // The gate owns the read and re-classifies the current bytes, so a file
        // the manifest still records as merely oversized cannot have a textual
        // match disclosed once its bytes turn sensitive. The refusal itself is
        // discarded: only the path reaches the response, via `unswept`.
        // T045: this is a DISK OBSERVATION by name — the sweep wants what is on
        // disk right now, confined beneath the root. Manifest paths are
        // relative and catalogued, so the confine never fires on them.
        match read(path, bytes_budget as usize) {
            Some(bytes) => {
                bytes_budget = bytes_budget.saturating_sub(bytes.len() as u64);
                if String::from_utf8_lossy(&bytes).contains(name) {
                    matched.push(path.as_str());
                }
            }
            None => unswept.push(path.as_str()),
        }
    }
    if matched.is_empty() && unswept.is_empty() {
        return None;
    }
    let mut parts: Vec<String> = Vec::new();
    if !matched.is_empty() {
        parts.push(format!(
            "\"{name}\" appears textually in {} size-demoted Tier-2 file(s) that were NOT \
             reference-scanned (metadata-only): {} — grep or get_file_content to inspect",
            matched.len(),
            preview(&matched),
        ));
    }
    if !unswept.is_empty() {
        parts.push(format!(
            "{} size-demoted Tier-2 file(s) not reference-scanned and not swept (budget or policy): {}",
            unswept.len(),
            preview(&unswept),
        ));
    }
    Some(format!("Tier-2 exclusion: {}", parts.join("; ")))
}

pub(crate) fn with_withheld_note(
    live: &LiveIndex,
    scope: Option<&str>,
    mut output: String,
) -> String {
    let scope = scope
        .map(super::read_gate::normalize_requested_path)
        .filter(|scope| !scope.is_empty());
    if let Some(note) = super::read_context::withheld_not_searched_note(
        live.withheld_since_restore(),
        scope.as_deref(),
    ) {
        output = format!("{note}\n\n{output}");
    }
    output
}
