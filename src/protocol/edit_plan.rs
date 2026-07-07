//! Edit planning: analyzes a target symbol/file and suggests the right
//! sequence of SymForge edit tools to accomplish a change.

use crate::domain::SymbolRecord;
use crate::live_index::query::{SymbolSelectorMatch, resolve_symbol_selector};
use crate::live_index::store::{IndexedFile, LiveIndex};

fn split_path_qualified_target(target: &str) -> Option<(&str, &str)> {
    if let Some((path, name)) = target.split_once("::") {
        let path = path.trim();
        let name = name.trim();
        if !path.is_empty() && !name.is_empty() {
            return Some((path, name));
        }
    }

    None
}

/// Plan an edit operation: analyze impact and suggest tool sequence.
///
/// Rendered symbol line ranges are one-based public selector lines. Internally
/// `SymbolRecord::line_range` remains zero-based.
///
/// `temporal` is the lock-free git temporal snapshot taken from the shared
/// index handle (it does not live on the `LiveIndex` read snapshot). When it is
/// `Ready` and the primary target file has strong co-change partners, the
/// symbol branch emits a single terse `Co-change partners: a, b, c` line;
/// otherwise nothing is emitted (clean silent omission, no placeholder).
pub fn plan_edit(
    index: &LiveIndex,
    temporal: &crate::live_index::git_temporal::GitTemporalIndex,
    target: &str,
) -> String {
    let target = target.trim();
    let qualified_target = split_path_qualified_target(target);

    // Try to find the target as a symbol first
    let mut symbol_hits = Vec::new();
    let mut file_hit = None;

    if let Some((target_path, target_name)) = qualified_target {
        for (path, file) in index.all_files() {
            if path == target_path || path.ends_with(target_path) {
                collect_selector_hits(&mut symbol_hits, path, file, target_name);
            }
            if path.ends_with(target) || path == target {
                file_hit = Some(path.clone());
            }
        }
    } else {
        for (path, file) in index.all_files() {
            collect_selector_hits(&mut symbol_hits, path, file, target);
            if path.ends_with(target) || path == target {
                file_hit = Some(path.clone());
            }
        }
    }

    let symbol_target_name = symbol_hits
        .first()
        .map(|(_, name, _, _)| name.as_str())
        .or_else(|| qualified_target.map(|(_, name)| name))
        .unwrap_or(target);

    let mut lines = vec!["── Edit Plan ──".to_string()];

    if symbol_hits.is_empty() && file_hit.is_none() {
        lines.push(format!("Target '{}' not found.", target));
        lines.push("Try: search_symbols(query=\"...\") to find the correct name.".to_string());
        return lines.join("\n");
    }

    if !symbol_hits.is_empty() {
        lines.push(format!(
            "Found {} symbol(s) matching '{}':",
            symbol_hits.len(),
            target
        ));
        for (path, name, kind, (start, end)) in &symbol_hits {
            let public_start = start.saturating_add(1);
            let public_end = end.saturating_add(1);
            lines.push(format!(
                "  {:?} {} in {} (lines {}-{})",
                kind, name, path, public_start, public_end
            ));
        }

        // Count references
        let ref_count = index
            .find_references_for_name(symbol_target_name, None, false)
            .len();
        lines.push(format!(
            "\nReferences: {} call sites across the project",
            ref_count
        ));

        if ref_count > 10 {
            lines.push(
                "⚠ HIGH IMPACT: >10 callers. Use batch_rename(dry_run=true) to preview."
                    .to_string(),
            );
        }

        // Terse co-change line for the primary target file. Reuses
        // `format::edit_impact_summary` (Ready-gated, forward-slash normalized,
        // strong co_changes only, top-K). When temporal is not Ready or the file
        // has no strong co-change partners the partner list is empty and we push
        // NOTHING — clean silent omission, no placeholder line (unlike
        // `analyze_file_impact`, `plan_edit` stays terse).
        if let Some((primary_path, _, _, _)) = symbol_hits.first() {
            let (_, partners) =
                crate::protocol::format::edit_impact_summary(index, temporal, primary_path);
            if !partners.is_empty() {
                lines.push(format!("Co-change partners: {}", partners.join(", ")));
            }
        }

        lines.push("\nSuggested tool sequence:".to_string());
        if symbol_hits.len() == 1 {
            let (path, name, _, _) = &symbol_hits[0];
            lines.push(format!("  1. get_symbol_context(name=\"{name}\", path=\"{path}\", bundle=true) — understand full context"));
            lines.push("  2. Choose edit approach:".to_string());
            lines.push(format!("     - Small change: edit_within_symbol(path=\"{path}\", name=\"{name}\", old_text=..., new_text=...)"));
            lines.push(format!("     - Full rewrite: replace_symbol_body(path=\"{path}\", name=\"{name}\", new_body=...)"));
            lines.push(format!("     - Rename: batch_rename(path=\"{path}\", name=\"{name}\", new_name=..., dry_run=true)"));
            lines.push(format!(
                "     - Delete: delete_symbol(path=\"{path}\", name=\"{name}\", dry_run=true)"
            ));
            lines.push(format!(
                "  3. analyze_file_impact(path=\"{path}\") — verify changes"
            ));
        } else {
            lines.push("  1. Use symbol_line to disambiguate the target".to_string());
            lines.push("  2. get_symbol_context(bundle=true) on the chosen symbol".to_string());
            lines.push("  3. batch_edit(dry_run=true) for multi-symbol changes".to_string());
        }
    }

    if let Some(path) = file_hit
        && symbol_hits.is_empty()
    {
        lines.push(format!("Found file: {}", path));
        lines.push("\nSuggested approach:".to_string());
        lines.push(format!(
            "  1. get_file_context(path=\"{path}\", sections=[\"outline\"]) — understand structure"
        ));
        lines.push(format!(
            "  2. get_symbol(path=\"{path}\", name=\"<target>\") — read specific symbols"
        ));
        lines.push("  3. Use edit_within_symbol or replace_symbol_body for changes".to_string());
        lines.push(format!(
            "  4. analyze_file_impact(path=\"{path}\") — verify"
        ));
    }

    lines.join("\n")
}

fn collect_selector_hits(
    symbol_hits: &mut Vec<(String, String, crate::domain::SymbolKind, (u32, u32))>,
    path: &str,
    file: &IndexedFile,
    selector: &str,
) {
    match resolve_symbol_selector(file, selector, None, None) {
        SymbolSelectorMatch::Selected(_, sym) => {
            push_symbol_hit(symbol_hits, path, sym);
        }
        SymbolSelectorMatch::Ambiguous(lines) => {
            for line in lines {
                let symbol_line = line + 1;
                if let SymbolSelectorMatch::Selected(_, sym) =
                    resolve_symbol_selector(file, selector, None, Some(symbol_line))
                {
                    push_symbol_hit(symbol_hits, path, sym);
                }
            }
        }
        SymbolSelectorMatch::NotFound => {}
    }
}

fn push_symbol_hit(
    symbol_hits: &mut Vec<(String, String, crate::domain::SymbolKind, (u32, u32))>,
    path: &str,
    sym: &SymbolRecord,
) {
    symbol_hits.push((path.to_string(), sym.name.clone(), sym.kind, sym.line_range));
}
