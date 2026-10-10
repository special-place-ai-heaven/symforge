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
