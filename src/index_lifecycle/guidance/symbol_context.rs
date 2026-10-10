//! Shared symbol-context renderers, trace capture, and reference-section evidence.
use super::file_read::{not_found_file, not_found_symbol_names};
use super::read_context::{
    ContextSourceAuthority, build_with_budget, format_context_envelope, parse_state_label,
};
use super::reference_read::{
    OutputLimits, find_dependents_compact_view, find_dependents_result_view,
    implementations_result_view,
};
use super::source::enforce_token_budget;
#[cfg(feature = "server")]
use crate::live_index::LiveIndex;
use crate::live_index::{
    ContextBundleFoundView, ContextBundleReferenceView, ContextBundleSectionView,
    ContextBundleView, ImplBlockSuggestionView, IndexedFile, TypeDependencyView,
};

/// Get full context bundle for a symbol: definition body + callers + callees + type usages.
///
/// Each section is capped at 20 entries with "...and N more" overflow.
#[cfg(feature = "server")]
pub fn context_bundle_result(
    index: &LiveIndex,
    path: &str,
    name: &str,
    kind_filter: Option<&str>,
) -> String {
    let view = index.capture_context_bundle_view(path, name, kind_filter, None);
    context_bundle_result_view(&view, "full")
}

#[cfg(feature = "server")]
pub fn context_bundle_result_view(view: &ContextBundleView, verbosity: &str) -> String {
    context_bundle_result_view_with_max_tokens(view, verbosity, None)
}

pub fn context_bundle_impl_suggestion_tip(view: &ContextBundleView) -> String {
    match view {
        ContextBundleView::Found(view) => format_impl_block_suggestions(view.as_ref()),
        _ => String::new(),
    }
}

/// Extract a compact callees section from a context bundle view for use in
/// default `get_symbol_context` mode (which otherwise only shows callers).
pub fn context_bundle_callees_text(view: &ContextBundleView) -> String {
    match view {
        ContextBundleView::Found(found) if found.callees.total_count > 0 => {
            format_context_bundle_section("Callees", &found.callees)
        }
        _ => String::new(),
    }
}

pub fn context_bundle_result_view_with_max_tokens(
    view: &ContextBundleView,
    verbosity: &str,
    max_tokens: Option<u64>,
) -> String {
    match view {
        ContextBundleView::FileNotFound { path } => not_found_file(path),
        ContextBundleView::AmbiguousSymbol {
            path,
            name,
            candidate_lines,
        } => format!(
            "Ambiguous symbol selector for {name} in {path}; pass `symbol_line` to disambiguate. Candidates: {}",
            candidate_lines
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ContextBundleView::SymbolNotFound {
            relative_path,
            symbol_names,
            name,
        } => not_found_symbol_names(relative_path, symbol_names, name),
        ContextBundleView::Found(view) => {
            render_context_bundle_found_with_max_tokens(view.as_ref(), verbosity, max_tokens)
        }
    }
}

fn render_context_bundle_found_with_max_tokens(
    view: &ContextBundleFoundView,
    verbosity: &str,
    max_tokens: Option<u64>,
) -> String {
    let (body, actual_level) = resolve_verbosity(
        &view.body,
        Some(verbosity),
        max_tokens,
        0.4, // allocate 40% of budget to body, leave room for deps
    );
    let mut output = format!(
        "{}\n[{}, {}:{}-{}, {} bytes]\n",
        body,
        view.kind_label,
        view.file_path,
        view.line_range.0 + 1,
        view.line_range.1 + 1,
        view.byte_count
    );
    if actual_level != "full" && actual_level != verbosity {
        output.push_str(&format!(
            "[adaptive verbosity: {} — body reduced to fit {} token budget]\n",
            actual_level,
            max_tokens.unwrap_or(0)
        ));
    }
    output.push_str(&format_context_bundle_section("Callers", &view.callers));
    output.push_str(&format_context_bundle_section("Callees", &view.callees));
    output.push_str(&format_unresolved_same_name_member_calls(
        &view.unresolved_same_name_member_calls,
    ));
    output.push_str(&format_context_bundle_section(
        "Type usages",
        &view.type_usages,
    ));
    match max_tokens {
        Some(max_tokens) => {
            let max_bytes = (max_tokens as usize).saturating_mul(4);
            if max_bytes > 0 && output.len() > max_bytes {
                let mut truncated = truncate_text_at_line_boundary(&output, max_bytes);
                truncated.push_str(&format_bundle_truncation_notice(max_tokens, None));
                if !view.implementation_suggestions.is_empty() {
                    truncated.push_str(&format_impl_block_suggestions(view));
                }
                return truncated;
            }

            let (dep_text, omitted) =
                format_type_dependencies_with_budget(&view.dependencies, max_bytes, output.len());
            output.push_str(&dep_text);
            if omitted > 0 {
                output.push_str(&format_bundle_truncation_notice(max_tokens, Some(omitted)));
            }
        }
        None => {
            if !view.dependencies.is_empty() {
                output.push_str(&format_type_dependencies(&view.dependencies));
            }
        }
    }
    if !view.implementation_suggestions.is_empty() {
        output.push_str(&format_impl_block_suggestions(view));
    }
    output
}

/// Format results of `trace_symbol`.
pub fn trace_symbol_result_view(
    view: &crate::live_index::TraceSymbolView,
    name: &str,
    verbosity: &str,
    max_tokens: Option<u64>,
) -> String {
    match view {
        crate::live_index::TraceSymbolView::FileNotFound { path } => not_found_file(path),
        crate::live_index::TraceSymbolView::AmbiguousSymbol {
            path,
            name,
            candidate_lines,
        } => format!(
            "Ambiguous symbol selector for {name} in {path}; pass `symbol_line` to disambiguate. Candidates: {}",
            candidate_lines
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        crate::live_index::TraceSymbolView::SymbolNotFound {
            relative_path,
            symbol_names,
            name,
        } => not_found_symbol_names(relative_path, symbol_names, name),
        crate::live_index::TraceSymbolView::Found(found) => {
            let mut output = render_context_bundle_found_with_max_tokens(
                &found.context_bundle,
                verbosity,
                max_tokens,
            );

            if !found.siblings.is_empty() {
                output.push_str(&format_siblings_with_budget(
                    &found.siblings,
                    output.len(),
                    max_tokens,
                ));
            }

            if !found.dependents.files.is_empty() {
                let dependents_fn = if verbosity == "full" {
                    find_dependents_result_view
                } else {
                    find_dependents_compact_view
                };
                let section = format!(
                    "\n\n{}",
                    dependents_fn(
                        &found.dependents,
                        &found.context_bundle.file_path,
                        &OutputLimits::default(),
                    )
                );
                append_budgeted_trace_section(
                    &mut output,
                    &section,
                    max_tokens,
                    "Dependents",
                    found.dependents.files.len(),
                );
            }

            if !found.implementations.entries.is_empty() {
                let section = format!(
                    "\n\n{}",
                    implementations_result_view(
                        &found.implementations,
                        name,
                        &OutputLimits::default(),
                    )
                );
                append_budgeted_trace_section(
                    &mut output,
                    &section,
                    max_tokens,
                    "Implementations",
                    found.implementations.entries.len(),
                );
            }

            if let Some(git) = &found.git_activity {
                let section = format_trace_git_activity(git);
                append_budgeted_trace_section(&mut output, &section, max_tokens, "Git activity", 1);
            }

            enforce_token_budget(output, max_tokens)
        }
    }
}

fn append_budgeted_trace_section(
    output: &mut String,
    section: &str,
    max_tokens: Option<u64>,
    label: &str,
    omitted_count: usize,
) {
    let Some(max_tokens) = max_tokens.filter(|tokens| *tokens > 0) else {
        output.push_str(section);
        return;
    };
    let max_bytes = (max_tokens as usize).saturating_mul(4);

    if output.len().saturating_add(section.len()) <= max_bytes {
        output.push_str(section);
        return;
    }

    let detail = format!("{label} section omitted {omitted_count} item(s).");
    output.push_str(&format!(
        "\n\n{}",
        token_truncation_notice(max_tokens, Some(detail.as_str()))
    ));
}

fn format_siblings_with_budget(
    siblings: &[crate::live_index::SiblingSymbolView],
    current_bytes: usize,
    max_tokens: Option<u64>,
) -> String {
    let Some(max_tokens) = max_tokens.filter(|tokens| *tokens > 0) else {
        return format_siblings(siblings, 0);
    };
    let max_bytes = (max_tokens as usize).saturating_mul(4);

    let mut section = "\nNearby siblings:".to_string();
    let mut shown = 0usize;
    for sib in siblings {
        let line = format!(
            "\n  {:<12} {:<30} {}-{}",
            sib.kind_label, sib.name, sib.line_range.0, sib.line_range.1
        );
        if current_bytes
            .saturating_add(section.len())
            .saturating_add(line.len())
            > max_bytes
        {
            break;
        }
        section.push_str(&line);
        shown += 1;
    }

    let omitted = siblings.len().saturating_sub(shown);
    if omitted > 0 {
        let detail = format!("{omitted} additional sibling(s) not shown.");
        section.push_str(&format!(
            "\n  {}",
            token_truncation_notice(max_tokens, Some(detail.as_str()))
        ));
    }

    section
}

use crate::index_lifecycle::guidance::symbol_read::format_siblings;

fn format_trace_git_activity(git: &crate::live_index::GitActivityView) -> String {
    let mut lines = vec![String::new()];
    lines.push(format!(
        "Git activity:  {} {:.2} ({})    {} commits, last {}",
        git.churn_bar, git.churn_score, git.churn_label, git.commit_count, git.last_relative,
    ));
    lines.push(format!(
        "  Last:  {} \"{}\" ({}, {})",
        git.last_hash, git.last_message, git.last_author, git.last_timestamp,
    ));
    if !git.owners.is_empty() {
        lines.push(format!("  Owners: {}", git.owners.join(", ")));
    }
    if !git.co_changes.is_empty() {
        lines.push("  Co-changes:".to_string());
        for (path, coupling, shared) in &git.co_changes {
            lines.push(format!(
                "    {}  ({:.2} coupling, {} shared commits)",
                path, coupling, shared,
            ));
        }
    }
    lines.join("\n")
}

fn format_context_bundle_section(title: &str, section: &ContextBundleSectionView) -> String {
    // Detect if this section has deduplicated entries (any occurrence_count > 1).
    let has_dedup = section.entries.iter().any(|e| e.occurrence_count > 1);

    let header =
        if has_dedup && section.unique_count > 0 && section.unique_count < section.total_count {
            format!(
                "\n{title} ({} total, {} unique):",
                section.total_count, section.unique_count
            )
        } else {
            format!("\n{title} ({}):", section.total_count)
        };

    let mut lines = vec![header];

    let mut external_count = 0usize;

    for entry in &section.entries {
        if is_external_symbol(&entry.display_name, &entry.file_path) {
            external_count += 1;
        }

        // Build the name part, appending ×N for deduplicated entries.
        let name_part = if entry.occurrence_count > 1 {
            format!("{} (×{})", entry.display_name, entry.occurrence_count)
        } else {
            entry.display_name.clone()
        };

        if let Some(enclosing) = &entry.enclosing {
            lines.push(format!(
                "  {:<30} {}:{}  {}",
                name_part, entry.file_path, entry.line_number, enclosing
            ));
        } else {
            lines.push(format!(
                "  {:<30} {}:{}",
                name_part, entry.file_path, entry.line_number
            ));
        }
    }

    if section.overflow_count > 0 {
        // Estimate external ratio from shown entries and extrapolate
        let shown = section.entries.len();
        let est_external = if shown > 0 {
            (external_count as f64 / shown as f64 * section.overflow_count as f64).round() as usize
        } else {
            0
        };
        let est_project = section.overflow_count.saturating_sub(est_external);
        if has_dedup {
            // For deduplicated sections, overflow is in unique callee names
            lines.push(format!(
                "  ...and {} more unique {}",
                section.overflow_count,
                title.to_lowercase()
            ));
        } else if est_external > 0 {
            lines.push(format!(
                "  ...and {} more {} ({} project, ~{} stdlib/framework)",
                section.overflow_count,
                title.to_lowercase(),
                est_project,
                est_external
            ));
        } else {
            lines.push(format!(
                "  ...and {} more {}",
                section.overflow_count,
                title.to_lowercase()
            ));
        }
    }

    lines.join("\n")
}

/// Render the SF-002 "unresolved same-name member calls" section.
///
/// These are `receiver.<target_name>()` calls made from inside the target
/// symbol's body where the receiver type could not be resolved — for example a
/// TypeScript `Controller.foo` whose body calls `this.service.foo()`. They are
/// surfaced under a clearly-labeled line so they are visibly distinct from the
/// exact `Callers`/`Callees` counts, which deliberately exclude them. Renders
/// nothing when empty so the common case keeps its existing output shape.
fn format_unresolved_same_name_member_calls(entries: &[ContextBundleReferenceView]) -> String {
    if entries.is_empty() {
        return String::new();
    }

    let mut lines = vec![format!(
        "\nUnresolved same-name member calls ({}) [receiver type unresolved; not counted as callers/callees]:",
        entries.len()
    )];

    for entry in entries {
        lines.push(format!(
            "  {:<30} {}:{}",
            entry.display_name, entry.file_path, entry.line_number
        ));
    }

    lines.join("\n")
}

/// Heuristic: classify a symbol reference as external (stdlib/framework) vs project-defined.
fn is_external_symbol(name: &str, file_path: &str) -> bool {
    // No file path means it's a builtin/external
    if file_path.is_empty() {
        return true;
    }
    // Common stdlib/framework patterns across languages
    let external_prefixes = [
        "std::",
        "core::",
        "alloc::",
        "System.",
        "Microsoft.",
        "java.",
        "javax.",
        "kotlin.",
        "android.",
        "console.",
        "JSON.",
        "Math.",
        "Object.",
        "Array.",
        "String.",
        "Promise.",
        "Map.",
        "Set.",
        "Error.",
    ];
    for prefix in &external_prefixes {
        if name.starts_with(prefix) {
            return true;
        }
    }
    // Single-word lowercase names that are very common builtins
    let common_builtins = [
        "println",
        "print",
        "eprintln",
        "format",
        "vec",
        "to_string",
        "clone",
        "unwrap",
        "expect",
        "push",
        "pop",
        "len",
        "is_empty",
        "iter",
        "map",
        "filter",
        "collect",
        "into",
        "from",
        "default",
        "new",
        "Add",
        "Sub",
        "Display",
        "Debug",
        "ToString",
        "log",
        "warn",
        "error",
        "info",
        "LogWarning",
        "LogError",
        "LogInformation",
        "Console",
    ];
    common_builtins.contains(&name)
}

/// Extract the full signature from a symbol body.
///
/// Handles common patterns: `fn foo(...)`, `pub struct Foo`, `class Bar`, etc.
/// Skips leading doc comments, then collects lines until the declaration is
/// complete (opening brace `{`, `where` clause, or terminating `;`).
/// Multi-line signatures are joined on one line with spaces, preserving
/// visibility, generic parameters, and return type.
use crate::index_lifecycle::guidance::source::extract_signature;

/// Extract the first doc-comment line from a symbol body.
///
/// Looks for `///`, `//!`, `/** ... */`, `# ...` (Python docstring-adjacent),
/// or `/* ... */` style comments immediately before/after the signature.
fn extract_first_doc_line(body: &str) -> Option<String> {
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Rust doc comments
        if let Some(rest) = trimmed.strip_prefix("///") {
            let doc = rest.trim();
            if !doc.is_empty() {
                return Some(doc.to_string());
            }
        }
        // Rust inner doc comments
        if let Some(rest) = trimmed.strip_prefix("//!") {
            let doc = rest.trim();
            if !doc.is_empty() {
                return Some(doc.to_string());
            }
        }
        // C-style block doc comments
        if let Some(rest) = trimmed.strip_prefix("/**") {
            let doc = rest.trim_end_matches("*/").trim();
            if !doc.is_empty() {
                return Some(doc.to_string());
            }
        }
        // XML doc comments (C#)
        if trimmed.starts_with("/// <summary>") || trimmed.starts_with("/// <remarks>") {
            continue; // skip XML tags, look for actual text
        }
        // Python/JS docstrings
        if trimmed.starts_with("\"\"\"") || trimmed.starts_with("'''") {
            let doc = trimmed
                .trim_start_matches("\"\"\"")
                .trim_start_matches("'''")
                .trim_end_matches("\"\"\"")
                .trim_end_matches("'''")
                .trim();
            if !doc.is_empty() {
                return Some(doc.to_string());
            }
        }
        // If we hit a non-comment line, stop looking
        if !trimmed.starts_with("//")
            && !trimmed.starts_with("/*")
            && !trimmed.starts_with('*')
            && !trimmed.starts_with('#')
        {
            break;
        }
    }
    None
}

/// Apply verbosity filter to a symbol body.
///
/// - `"summary"`: one-line natural language summary (doc comment or heuristic from name/signature).
/// - `"signature"`: full declaration line — visibility, name, generics, params, return type (~80% smaller).
/// - `"compact"`: signature + first doc-comment line.
/// - `"full"` or anything else: complete body (default).
pub(crate) fn apply_verbosity(body: &str, verbosity: &str) -> String {
    match verbosity {
        "summary" => auto_summarize(body),
        "signature" => extract_signature(body),
        "compact" => {
            let sig = extract_signature(body);
            if let Some(doc) = extract_first_doc_line(body) {
                format!("{sig}\n  // {doc}")
            } else {
                sig
            }
        }
        _ => body.to_string(),
    }
}

/// Auto-select the richest verbosity level whose output fits within a token budget.
///
/// Cascades through `full → compact → signature → summary`, returning the first
/// level whose rendered output is ≤ `body_budget_tokens * 4` bytes.  Always returns
/// at least the summary level even if it exceeds the budget.
///
/// Returns `(rendered_body, level_name)`.
pub(crate) fn adaptive_verbosity(body: &str, body_budget_tokens: u64) -> (String, &'static str) {
    let max_bytes = (body_budget_tokens as usize).saturating_mul(4);

    // Try full first
    if body.len() <= max_bytes {
        return (body.to_string(), "full");
    }

    // Try compact
    let compact = apply_verbosity(body, "compact");
    if compact.len() <= max_bytes {
        return (compact, "compact");
    }

    // Try signature
    let signature = apply_verbosity(body, "signature");
    if signature.len() <= max_bytes {
        return (signature, "signature");
    }

    // Summary as last resort
    (apply_verbosity(body, "summary"), "summary")
}

/// Resolve verbosity for a symbol body, with adaptive fallback.
///
/// - If `explicit_verbosity` is `Some` (user chose a level), applies it directly.
/// - If `max_tokens` is `Some` and no explicit verbosity, auto-selects the richest
///   level fitting within `max_tokens * body_fraction`.
/// - Otherwise returns the full body.
///
/// `body_fraction` controls how much of the total token budget is allocated to the
/// body (e.g. 0.4 for bundle mode where dependencies need space, 0.7 for standalone).
///
/// Returns `(rendered_body, level_name)`.
pub(crate) fn resolve_verbosity(
    body: &str,
    explicit_verbosity: Option<&str>,
    max_tokens: Option<u64>,
    body_fraction: f64,
) -> (String, &'static str) {
    // Explicit user choice always wins
    if let Some(v) = explicit_verbosity
        && v != "full"
    {
        let level: &'static str = match v {
            "summary" => "summary",
            "signature" => "signature",
            "compact" => "compact",
            _ => "full",
        };
        return (apply_verbosity(body, v), level);
    }

    // Adaptive: auto-select when budget is set and no explicit verbosity (or explicit "full")
    if let Some(tokens) = max_tokens
        && tokens > 0
    {
        let body_budget = (tokens as f64 * body_fraction) as u64;
        let (rendered, level) = adaptive_verbosity(body, body_budget);
        if level != "full" {
            return (rendered, level);
        }
    }

    // Default: full
    (body.to_string(), "full")
}

/// Generate a one-line natural language summary for a symbol body.
///
/// Priority:
/// 1. First doc-comment line (if present and meaningful)
/// 2. Heuristic from function name patterns (get_, set_, is_, new, from_, etc.)
/// 3. Signature-based fallback with parameter/return type info
pub(crate) fn auto_summarize(body: &str) -> String {
    // Try doc comment first
    if let Some(doc) = extract_first_doc_line(body) {
        // Ensure it's meaningful (not just a tag or very short)
        if doc.len() > 5 && !doc.starts_with('@') && !doc.starts_with('<') {
            return doc;
        }
    }

    let sig = extract_signature(body);

    // Extract the function/type name from the signature
    let name = extract_declaration_name(&sig).unwrap_or_default();
    if name.is_empty() {
        return sig;
    }

    // Try heuristic summary based on name patterns
    if let Some(heuristic) = heuristic_from_name(&name, &sig) {
        return heuristic;
    }

    // Fallback: signature-based summary
    sig
}

/// Generate a heuristic summary from common naming patterns.
pub(crate) fn heuristic_from_name(name: &str, sig: &str) -> Option<String> {
    let lower = name.to_ascii_lowercase();

    // Common prefixes with semantic meaning
    let patterns: &[(&str, &str)] = &[
        ("test_", "Test: "),
        ("get_", "Returns the "),
        ("set_", "Sets the "),
        ("is_", "Checks whether "),
        ("has_", "Checks whether it has "),
        ("should_", "Checks whether it should "),
        ("can_", "Checks whether it can "),
        ("with_", "Creates a copy with "),
        ("from_", "Constructs from "),
        ("into_", "Converts into "),
        ("try_", "Attempts to "),
        ("parse_", "Parses "),
        ("render_", "Renders "),
        ("format_", "Formats "),
        ("validate_", "Validates "),
        ("build_", "Builds "),
        ("create_", "Creates "),
        ("make_", "Creates "),
        ("load_", "Loads "),
        ("save_", "Saves "),
        ("read_", "Reads "),
        ("write_", "Writes "),
        ("find_", "Finds "),
        ("search_", "Searches for "),
        ("collect_", "Collects "),
        ("compute_", "Computes "),
        ("calculate_", "Calculates "),
        ("update_", "Updates "),
        ("delete_", "Deletes "),
        ("remove_", "Removes "),
        ("add_", "Adds "),
        ("insert_", "Inserts "),
        ("handle_", "Handles "),
        ("process_", "Processes "),
        ("run_", "Runs "),
        ("execute_", "Executes "),
        ("start_", "Starts "),
        ("stop_", "Stops "),
        ("init_", "Initializes "),
        ("setup_", "Sets up "),
        ("cleanup_", "Cleans up "),
        ("resolve_", "Resolves "),
        ("normalize_", "Normalizes "),
        ("convert_", "Converts "),
        ("transform_", "Transforms "),
        ("apply_", "Applies "),
        ("check_", "Checks "),
        ("ensure_", "Ensures "),
        ("spawn_", "Spawns "),
        ("emit_", "Emits "),
        ("dispatch_", "Dispatches "),
        ("register_", "Registers "),
        ("detect_", "Detects "),
        ("extract_", "Extracts "),
        ("capture_", "Captures "),
        ("record_", "Records "),
    ];

    for (prefix, verb) in patterns {
        if lower.starts_with(prefix) {
            let rest = &name[prefix.len()..];
            let readable = rest.replace('_', " ");
            let mut chars = readable.chars();
            let capitalized = match chars.next() {
                Some(first) => {
                    let mut text = first.to_uppercase().collect::<String>();
                    text.push_str(chars.as_str());
                    text
                }
                None => readable,
            };
            return Some(format!("{verb}{capitalized}"));
        }
    }

    // Special cases
    if lower == "new" || lower == "default" {
        // Check if it's inside an impl block
        if sig.contains("impl") || sig.contains("Self") || sig.contains("->") {
            return Some("Constructor".to_string());
        }
    }

    if lower == "drop" {
        return Some("Destructor / cleanup on drop".to_string());
    }

    if lower == "fmt" && sig.contains("Formatter") {
        return Some("Display/Debug formatting implementation".to_string());
    }

    // Struct/enum/type with field count
    if sig.contains("struct ") || sig.contains("class ") {
        return Some(format!("Data type: {name}"));
    }
    if sig.contains("enum ") {
        return Some(format!("Enumeration: {name}"));
    }
    if sig.contains("trait ") || sig.contains("interface ") {
        return Some(format!("Interface/trait: {name}"));
    }
    if sig.contains("impl ") {
        return Some(format!("Implementation block for {name}"));
    }

    None
}

pub(crate) fn format_type_dependencies(deps: &[TypeDependencyView]) -> String {
    let mut output = format!("\nDependencies ({}):", deps.len());
    for dep in deps {
        output.push_str(&format_type_dependency(dep));
    }
    output
}

fn format_type_dependencies_with_budget(
    deps: &[TypeDependencyView],
    max_bytes: usize,
    base_len: usize,
) -> (String, usize) {
    if deps.is_empty() || max_bytes == 0 {
        return (String::new(), 0);
    }

    let mut rendered = String::new();
    let header = format!("\nDependencies ({}):", deps.len());
    let mut header_added = false;
    let mut included = 0usize;

    for dep in deps {
        let dep_block = format_type_dependency(dep);
        let header_cost = if header_added { 0 } else { header.len() };
        if base_len + rendered.len() + header_cost + dep_block.len() > max_bytes {
            break;
        }
        if !header_added {
            rendered.push_str(&header);
            header_added = true;
        }
        rendered.push_str(&dep_block);
        included += 1;
    }

    if included == deps.len() {
        return (rendered, 0);
    }
    if included == 0 {
        return (String::new(), deps.len());
    }
    (rendered, deps.len().saturating_sub(included))
}

fn format_type_dependency(dep: &TypeDependencyView) -> String {
    let depth_marker = if dep.depth > 0 {
        format!(" (depth {})", dep.depth)
    } else {
        String::new()
    };
    format!(
        "\n── {} [{}, {}:{}-{}{}] ──\n{}",
        dep.name,
        dep.kind_label,
        dep.file_path,
        dep.line_range.0 + 1,
        dep.line_range.1 + 1,
        depth_marker,
        dep.body
    )
}

fn format_impl_block_suggestions(view: &ContextBundleFoundView) -> String {
    let is_type_definition = matches!(view.kind_label.as_str(), "struct" | "enum");
    if !is_type_definition
        || view.callers.total_count != 0
        || view.implementation_suggestions.is_empty()
    {
        return String::new();
    }

    let mut output = format!(
        "\nTip: This {} has 0 direct callers. Try `get_symbol_context` on one of its impl blocks:",
        view.kind_label
    );
    for suggestion in &view.implementation_suggestions {
        output.push_str(&format_impl_block_suggestion(suggestion));
    }
    output.push('\n');
    output
}

fn format_impl_block_suggestion(suggestion: &ImplBlockSuggestionView) -> String {
    format!(
        "\n- {} ({}:{})",
        suggestion.display_name, suggestion.file_path, suggestion.line_number
    )
}

use crate::index_lifecycle::guidance::source::{
    token_truncation_footer, token_truncation_notice, truncate_text_at_line_boundary,
};

fn format_bundle_truncation_notice(max_tokens: u64, omitted_dependencies: Option<usize>) -> String {
    let detail = omitted_dependencies
        .map(|count| format!("{count} additional type dependencies not shown."));
    token_truncation_footer(max_tokens, detail.as_deref())
}

fn is_likely_type_keyword(word: &str) -> bool {
    matches!(
        word,
        "string"
            | "String"
            | "int"
            | "Int32"
            | "Int64"
            | "bool"
            | "Boolean"
            | "float"
            | "double"
            | "decimal"
            | "char"
            | "byte"
            | "long"
            | "short"
            | "uint"
            | "object"
            | "var"
            | "number"
            | "bigint"
            | "any"
    )
}

pub(crate) fn extract_declaration_name(line: &str) -> Option<String> {
    // Strip leading visibility modifier generically: pub, pub(crate), pub(super), pub(in path).
    let stripped = if let Some(rest) = line.strip_prefix("pub") {
        if let Some(after_paren) = rest.strip_prefix('(') {
            // Skip balanced parens: pub(crate), pub(super), pub(in crate::foo)
            if let Some(close) = after_paren.find(')') {
                after_paren[close + 1..].trim_start()
            } else {
                rest.trim_start()
            }
        } else {
            rest.trim_start()
        }
    } else if let Some(rest) = line.strip_prefix("export default ") {
        rest
    } else if let Some(rest) = line.strip_prefix("export ") {
        rest
    } else {
        line
    };

    let keywords = [
        "async fn ",
        "fn ",
        "struct ",
        "enum ",
        "trait ",
        "type ",
        "const ",
        "static ",
        "class ",
        "interface ",
        "function ",
        "async function ",
        "async def ",
        "def ",
    ];

    for kw in &keywords {
        if let Some(rest) = stripped.strip_prefix(kw) {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if name.is_empty() {
                continue;
            }
            // For `const`, the first word might be a type name (C#: `const string Foo`).
            // If it looks like a well-known type, skip it and take the next identifier.
            if *kw == "const " && is_likely_type_keyword(&name) {
                let after_type = &rest[name.len()..].trim_start();
                let real_name: String = after_type
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if !real_name.is_empty() {
                    return Some(real_name);
                }
            }
            return Some(name);
        }
    }
    None
}

pub(crate) fn context_bundle_completeness_label(
    view: &crate::live_index::ContextBundleFoundView,
    rendered: &str,
) -> String {
    let section_overflow =
        view.callers.overflow_count + view.callees.overflow_count + view.type_usages.overflow_count;
    let mut parts = Vec::new();
    if rendered.contains("Truncated at ~") {
        parts.push("budget-limited".to_string());
    }
    if section_overflow > 0 {
        parts.push(format!(
            "section-capped ({} additional reference entries not shown)",
            section_overflow
        ));
    }
    if parts.is_empty() {
        "full".to_string()
    } else {
        parts.join("; ")
    }
}

pub(crate) fn format_ambiguous_symbol_context(name: &str, candidates: &[String]) -> String {
    let mut output = format!(
        "Ambiguous symbol selector: {} symbols named \"{}\" found.\nPass `path` or `file` to disambiguate.\nCandidate paths:",
        candidates.len(),
        name
    );
    for path in candidates {
        output.push_str(&format!("\n- {path}"));
    }
    output
}

pub(crate) fn render_symbol_context_header(
    file: &IndexedFile,
    name: &str,
    symbol_kind: Option<&str>,
    symbol_line: Option<u32>,
    verbosity: &str,
    max_tokens: Option<u64>,
) -> Option<String> {
    use crate::live_index::query::{SymbolSelectorMatch, resolve_symbol_selector};

    match resolve_symbol_selector(file, name, symbol_kind, symbol_line) {
        SymbolSelectorMatch::Selected(_, sym) => {
            let body = std::str::from_utf8(
                &file.content[sym.byte_range.0 as usize..sym.byte_range.1 as usize],
            )
            .ok()?;
            let (rendered, actual_level) = resolve_verbosity(
                body,
                Some(verbosity),
                max_tokens,
                0.7, // standalone symbol — allocate 70% of budget to body
            );
            let mut output = format!(
                "{}\n[{}, {}:{}-{}]",
                rendered,
                sym.kind,
                file.relative_path,
                sym.line_range.0 + 1,
                sym.line_range.1 + 1
            );
            if actual_level != "full" && actual_level != verbosity {
                output.push_str(&format!(
                    "\n[adaptive verbosity: {} — fits within {} token budget]",
                    actual_level,
                    max_tokens.unwrap_or(0)
                ));
            }
            Some(output)
        }
        SymbolSelectorMatch::NotFound | SymbolSelectorMatch::Ambiguous(_) => None,
    }
}

pub(crate) fn capture_trace_symbol_view(
    published: &crate::live_index::PublishedGeneration,
    path: &str,
    name: &str,
    kind: Option<&str>,
    symbol_line: Option<u32>,
    sections: Option<&[String]>,
    include_tests: bool,
) -> crate::live_index::TraceSymbolView {
    let mut trace_view = published.live.capture_trace_symbol_view(
        path,
        name,
        kind,
        symbol_line,
        sections,
        include_tests,
    );

    if let crate::live_index::TraceSymbolView::Found(ref mut found) = trace_view {
        let wants_git = sections
            .map(|values| values.iter().any(|value| value.eq_ignore_ascii_case("git")))
            .unwrap_or(true);

        if wants_git {
            let temporal = &published.code_signals.temporal;
            if temporal.state == crate::live_index::git_temporal::GitTemporalState::Ready
                && let Some(history) = temporal.files.get(path)
            {
                use crate::live_index::git_temporal::{churn_bar, churn_label, relative_time};

                found.git_activity = Some(crate::live_index::GitActivityView {
                    churn_score: history.churn_score,
                    churn_bar: churn_bar(history.churn_score),
                    churn_label: churn_label(history.churn_score).to_string(),
                    commit_count: history.commit_count,
                    last_relative: relative_time(history.last_commit.days_ago),
                    last_hash: history.last_commit.hash.clone(),
                    last_message: history.last_commit.message_head.clone(),
                    last_author: history.last_commit.author.clone(),
                    last_timestamp: history.last_commit.timestamp.clone(),
                    owners: history
                        .contributors
                        .iter()
                        .map(|contributor| {
                            format!("{} {:.0}%", contributor.author, contributor.percentage)
                        })
                        .collect(),
                    co_changes: history
                        .co_changes
                        .iter()
                        .map(|entry| {
                            (
                                entry.path.clone(),
                                entry.coupling_score,
                                entry.shared_commits,
                            )
                        })
                        .collect(),
                });
            }
        }
    }

    trace_view
}

pub(crate) struct SymbolContextSelector<'a> {
    pub name: &'a str,
    pub file: Option<&'a str>,
    pub path: Option<&'a str>,
    pub symbol_kind: Option<&'a str>,
    pub symbol_line: Option<u32>,
}

pub(crate) fn symbol_context_references(
    published: &crate::live_index::PublishedGeneration,
    params: &SymbolContextSelector<'_>,
    references_budget_bytes: u64,
    max_reference_entries: usize,
    source_authority: ContextSourceAuthority,
) -> (String, u64, usize) {
    let guard = published.live.as_ref();

    let references = if let Some(path) = params.path {
        match guard.find_exact_references_for_symbol(
            path,
            params.name,
            params.symbol_kind,
            params.symbol_line,
            None,
        ) {
            Ok(refs) => refs,
            Err(error) => return (error, 0, 0),
        }
    } else {
        guard.find_references_for_name(params.name, None, false)
    };

    // Group by file, applying optional file filter, capping at 10 total matches.
    let mut map: std::collections::HashMap<String, Vec<(u32, String, Option<String>)>> =
        std::collections::HashMap::new();

    let mut total = 0usize;
    let mut grand_total = 0usize;

    for (file_path, reference) in &references {
        grand_total += 1;
        if let Some(filter_file) = params.file
            && *file_path != filter_file
        {
            continue;
        }
        if total >= max_reference_entries {
            continue; // count beyond 10 but don't include
        }

        // Capture the enclosing symbol as a kind-aware display label
        // (e.g. "impl BucketManager", "struct BucketManager", "fn delta")
        // instead of bare name, so the reference list does not mislabel every
        // enclosing symbol as a function.
        let enclosing = reference.enclosing_symbol_index.and_then(|idx| {
            guard
                .get_file(file_path)
                .and_then(|f| f.symbols.get(idx as usize))
                .map(|s| super::symbol_read::symbol_kind_name_label(&s.kind.to_string(), &s.name))
        });

        map.entry(file_path.to_string()).or_default().push((
            reference.line_range.0,
            format!("{}", reference.kind),
            enclosing,
        ));
        total += 1;
    }

    // Compute total bytes for savings (sum of content of all matched files).
    let total_bytes: u64 = map
        .keys()
        .filter_map(|fp| guard.get_file(fp))
        .map(|f| f.byte_len)
        .sum();

    let parse_state = if let Some(path) = params.path {
        guard
            .get_file(path)
            .map(parse_state_label)
            .unwrap_or_else(|| {
                aggregate_parse_state_label(std::iter::empty(), published.health.as_ref())
            })
    } else if let Some(file) = params.file {
        guard
            .get_file(file)
            .map(parse_state_label)
            .unwrap_or_else(|| {
                aggregate_parse_state_label(std::iter::empty(), published.health.as_ref())
            })
    } else {
        aggregate_parse_state_label(
            map.keys()
                .filter_map(|file_path| guard.get_file(file_path))
                .map(|file| &file.parse_status),
            published.health.as_ref(),
        )
    };

    // Sort files for deterministic output.
    let mut files: Vec<String> = map.keys().cloned().collect();
    files.sort();

    let mut evidence_anchors: Vec<String> = Vec::new();
    // Files that contributed at least one anchor. The 3-anchor cap can exhaust
    // on the first file(s); the evidence line must then say how many reference
    // files it left unnamed instead of silently undercounting usage sites.
    let mut anchored_files = 0usize;
    for file in &files {
        // safe: `files` is built from `map.keys()` immediately above; lookup cannot miss.
        let refs = map.get(file).unwrap();
        let mut sorted_refs = refs.clone();
        sorted_refs.sort_by_key(|(line, _, _)| *line);
        let before = evidence_anchors.len();
        for (line, _, _) in &sorted_refs {
            if evidence_anchors.len() >= 3 {
                break;
            }
            evidence_anchors.push(format!("{file}:{line}"));
        }
        if evidence_anchors.len() > before {
            anchored_files += 1;
        }
        if evidence_anchors.len() >= 3 {
            break;
        }
    }

    let mut body_lines: Vec<String> = Vec::new();

    for file in &files {
        body_lines.push(format!("── {} ──", file));
        // safe: `files` is built from `map.keys()` above; lookup cannot miss.
        let refs = map.get(file).unwrap();
        let mut sorted_refs = refs.clone();
        sorted_refs.sort_by_key(|(line, _, _)| *line);
        for (line, _kind, enclosing) in &sorted_refs {
            if let Some(sym_label) = enclosing {
                body_lines.push(format!("  line {}  in {}", line, sym_label));
            } else {
                body_lines.push(format!("  line {}  (module level)", line));
            }
        }
    }

    if body_lines.is_empty() {
        // Dogfood #8 (2026-07-06): hooks feed this into prompt context on
        // every Grep, so a zero-hit report must cost one line.
        body_lines.push(
            "No references found in the index (not a symbol, or only dynamic/external usage)."
                .to_string(),
        );
    }

    if total < grand_total {
        if params.file.is_some() {
            body_lines.push(format!(
                "... (showing {} of {} matches — use `path` to narrow further)",
                total, grand_total
            ));
        } else {
            body_lines.push(format!(
                "... (showing {} of {} matches — use `path` or `file` to narrow)",
                total, grand_total
            ));
        }
    }

    // Apply the references-section budget. Tool calls get ~1000 tokens
    // (4000 bytes); the prompt-context hook stays at ~100 tokens (400 bytes).
    let (body_text, remaining) = build_with_budget(&body_lines, references_budget_bytes);
    let completeness = if total < grand_total {
        "truncated"
    } else if remaining > 0 {
        "budget-limited"
    } else {
        "full"
    };
    let match_type = if params.path.is_some() && params.symbol_line.is_some() {
        "exact"
    } else if params.path.is_some() || params.file.is_some() {
        "constrained"
    } else {
        "heuristic"
    };
    let evidence = if let Some(path) = params.path {
        match params.symbol_line {
            Some(line) => format!(
                "exact selector `{path}:{line}` for symbol `{}`",
                params.name
            ),
            None => format!("path-constrained symbol `{}` in `{path}`", params.name),
        }
    } else if let Some(file) = params.file {
        format!("file filter `{file}` for symbol `{}`", params.name)
    } else if evidence_anchors.is_empty() {
        format!(
            "symbol token `{}` with no indexed reference anchors",
            params.name
        )
    } else {
        let more_files = files.len().saturating_sub(anchored_files);
        if more_files > 0 {
            format!(
                "symbol token `{}` anchored at {} (+{} more files)",
                params.name,
                evidence_anchors.join(", "),
                more_files
            )
        } else {
            format!(
                "symbol token `{}` anchored at {}",
                params.name,
                evidence_anchors.join(", ")
            )
        }
    };
    let scope = if let Some(path) = params.path {
        match params.symbol_line {
            Some(line) => format!("path `{path}`; exact selector line {line}"),
            None => format!("path `{path}`; symbol-scoped references"),
        }
    } else if let Some(file) = params.file {
        format!("file filter `{file}`; symbol token `{}`", params.name)
    } else {
        format!("repo-wide symbol token `{}`", params.name)
    };
    let envelope = format_context_envelope(
        match_type,
        source_authority,
        parse_state,
        completeness,
        scope,
        evidence,
    );
    let text = format!("{envelope}\n\n{body_text}");

    (text, total_bytes, total)
}

fn aggregate_parse_state_label<'a>(
    statuses: impl IntoIterator<Item = &'a crate::live_index::store::ParseStatus>,
    published: &crate::live_index::store::PublishedIndexState,
) -> &'static str {
    let mut saw_partial = false;
    for status in statuses {
        match status {
            crate::live_index::store::ParseStatus::Parsed => {}
            crate::live_index::store::ParseStatus::PartialParse { .. } => saw_partial = true,
            crate::live_index::store::ParseStatus::Failed { .. } => return "degraded",
        }
    }
    if saw_partial {
        "partial"
    } else if matches!(
        published.status,
        crate::live_index::store::PublishedIndexStatus::Degraded
    ) {
        "degraded"
    } else {
        "parsed"
    }
}
