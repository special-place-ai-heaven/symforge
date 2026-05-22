use std::collections::HashSet;

use crate::domain::{ReferenceKind, ReferenceRecord, SymbolKind};

use super::disambiguation::{SymbolSelectorMatch, resolve_symbol_selector};
use super::query::is_filtered_name;
use super::store::LiveIndex;
/// One compact reference entry rendered inside a context-bundle section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextBundleReferenceView {
    pub display_name: String,
    pub file_path: String,
    pub line_number: u32,
    pub enclosing: Option<String>,
    /// When callees are deduplicated by name, this holds the total call-site count.
    /// Defaults to 1 for non-deduplicated entries.
    pub occurrence_count: usize,
}

/// One owned section inside a captured context bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextBundleSectionView {
    pub total_count: usize,
    pub overflow_count: usize,
    pub entries: Vec<ContextBundleReferenceView>,
    /// Number of unique symbol names in the full (uncapped) set.
    /// When deduplication was applied, `unique_count < total_count`.
    pub unique_count: usize,
}

/// Suggested impl block to inspect when a type definition has no direct callers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplBlockSuggestionView {
    pub display_name: String,
    pub file_path: String,
    pub line_number: u32,
}

/// A resolved type definition included as a dependency of a context bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeDependencyView {
    /// Type name (e.g. "UserConfig").
    pub name: String,
    /// Kind label (e.g. "struct", "enum", "trait").
    pub kind_label: String,
    /// File where the type is defined.
    pub file_path: String,
    /// Line range of the definition.
    pub line_range: (u32, u32),
    /// Source code body of the definition.
    pub body: String,
    /// Recursion depth at which this dependency was discovered (0 = direct, 1 = transitive).
    pub depth: u8,
}

/// Owned definition-and-sections view for bundle-mode `get_symbol_context`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextBundleFoundView {
    pub file_path: String,
    pub body: String,
    pub kind_label: String,
    pub line_range: (u32, u32),
    pub byte_count: usize,
    pub callers: ContextBundleSectionView,
    pub callees: ContextBundleSectionView,
    pub type_usages: ContextBundleSectionView,
    /// Resolved type definitions used by this symbol (recursive, depth-limited).
    pub dependencies: Vec<TypeDependencyView>,
    /// Suggested impl blocks for struct/enum symbols with no direct callers.
    pub implementation_suggestions: Vec<ImplBlockSuggestionView>,
}

/// Owned result view for bundle-mode `get_symbol_context`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextBundleView {
    FileNotFound {
        path: String,
    },
    AmbiguousSymbol {
        path: String,
        name: String,
        candidate_lines: Vec<u32>,
    },
    SymbolNotFound {
        relative_path: String,
        symbol_names: Vec<String>,
        name: String,
    },
    Found(Box<ContextBundleFoundView>),
}

impl LiveIndex {
    /// Capture the full owned data needed for `get_symbol_context` bundle mode.
    pub fn capture_context_bundle_view(
        &self,
        path: &str,
        name: &str,
        kind_filter: Option<&str>,
        symbol_line: Option<u32>,
    ) -> ContextBundleView {
        use crate::domain::ReferenceKind;

        const CONTEXT_BUNDLE_SECTION_CAP: usize = 20;

        let Some(file) = self.get_file(path) else {
            return ContextBundleView::FileNotFound {
                path: path.to_string(),
            };
        };

        let (sym_idx, sym_rec) = match resolve_symbol_selector(file, name, kind_filter, symbol_line)
        {
            SymbolSelectorMatch::Selected(sym_idx, sym_rec) => (sym_idx, sym_rec),
            SymbolSelectorMatch::NotFound => {
                return ContextBundleView::SymbolNotFound {
                    relative_path: file.relative_path.clone(),
                    symbol_names: file
                        .symbols
                        .iter()
                        .map(|symbol| symbol.name.clone())
                        .collect(),
                    name: name.to_string(),
                };
            }
            SymbolSelectorMatch::Ambiguous(candidate_lines) => {
                return ContextBundleView::AmbiguousSymbol {
                    path: file.relative_path.clone(),
                    name: name.to_string(),
                    candidate_lines,
                };
            }
        };

        let start = sym_rec.effective_start() as usize;
        let end = sym_rec.item_end() as usize;
        let clamped_end = end.min(file.content.len());
        let clamped_start = start.min(clamped_end);
        let body = String::from_utf8_lossy(&file.content[clamped_start..clamped_end]).into_owned();
        let byte_count = end.saturating_sub(start);

        let capture_section = |refs: &[(&str, &ReferenceRecord)]| -> ContextBundleSectionView {
            let entries: Vec<ContextBundleReferenceView> = refs
                .iter()
                .take(CONTEXT_BUNDLE_SECTION_CAP)
                .map(|(file_path, reference)| {
                    let enclosing = self.get_file(file_path).and_then(|f| {
                        reference
                            .enclosing_symbol_index
                            .and_then(|idx| f.symbols.get(idx as usize))
                            .map(|symbol| format!("in {} {}", symbol.kind, symbol.name))
                    });

                    ContextBundleReferenceView {
                        display_name: reference
                            .qualified_name
                            .as_deref()
                            .unwrap_or(&reference.name)
                            .to_string(),
                        file_path: (*file_path).to_string(),
                        line_number: reference.line_range.0 + 1,
                        enclosing,
                        occurrence_count: 1,
                    }
                })
                .collect();

            let unique_count = {
                let mut names: Vec<&str> = refs.iter().map(|(_, r)| r.name.as_str()).collect();
                names.sort_unstable();
                names.dedup();
                names.len()
            };

            ContextBundleSectionView {
                total_count: refs.len(),
                overflow_count: refs.len().saturating_sub(entries.len()),
                entries,
                unique_count,
            }
        };

        // Maximum number of unique callee names to show in a deduplicated section.
        const CALLEE_UNIQUE_CAP: usize = 30;

        let capture_callee_section =
            |refs: &[(&str, &ReferenceRecord)]| -> ContextBundleSectionView {
                // Group callees by name (short name, not qualified) and count occurrences.
                let mut name_counts: std::collections::HashMap<&str, (usize, usize)> =
                    std::collections::HashMap::new();
                for (idx, (_file_path, reference)) in refs.iter().enumerate() {
                    let entry = name_counts
                        .entry(reference.name.as_str())
                        .or_insert((0, idx));
                    entry.0 += 1;
                }

                // Sort by frequency (descending), then alphabetically for ties.
                let mut sorted_names: Vec<(&str, usize, usize)> = name_counts
                    .into_iter()
                    .map(|(name, (count, first_idx))| (name, count, first_idx))
                    .collect();
                sorted_names.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));

                let unique_total = sorted_names.len();
                let capped = &sorted_names[..sorted_names.len().min(CALLEE_UNIQUE_CAP)];

                let entries: Vec<ContextBundleReferenceView> = capped
                    .iter()
                    .map(|(name, count, first_idx)| {
                        let (file_path, reference) = &refs[*first_idx];
                        let enclosing = self.get_file(file_path).and_then(|f| {
                            reference
                                .enclosing_symbol_index
                                .and_then(|idx| f.symbols.get(idx as usize))
                                .map(|symbol| format!("in {} {}", symbol.kind, symbol.name))
                        });

                        ContextBundleReferenceView {
                            display_name: reference
                                .qualified_name
                                .as_deref()
                                .unwrap_or(name)
                                .to_string(),
                            file_path: (*file_path).to_string(),
                            line_number: reference.line_range.0 + 1,
                            enclosing,
                            occurrence_count: *count,
                        }
                    })
                    .collect();

                let overflow_unique = unique_total.saturating_sub(capped.len());

                ContextBundleSectionView {
                    total_count: refs.len(),
                    overflow_count: overflow_unique,
                    entries,
                    unique_count: unique_total,
                }
            };

        let callers =
            self.collect_exact_symbol_references(path, file, sym_rec, Some(ReferenceKind::Call));
        let callees = self.callees_for_symbol(path, sym_idx);
        let callee_pairs: Vec<(&str, &ReferenceRecord)> =
            callees.iter().map(|reference| (path, *reference)).collect();
        let type_usages = self.collect_exact_symbol_references(
            path,
            file,
            sym_rec,
            Some(ReferenceKind::TypeUsage),
        );

        // Resolve type dependencies: collect type names referenced within this symbol,
        // then find their definitions across the index (recursive, depth-limited to 2).
        let type_refs = self.type_refs_for_symbol(path, sym_idx);
        let type_names: Vec<&str> = type_refs
            .iter()
            .map(|r| r.name.as_str())
            // Exclude the target symbol's own name to avoid self-referential dependencies.
            .filter(|n| *n != name)
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        let dependencies = self.resolve_type_dependencies(&type_names, 2);
        let implementation_suggestions = if matches!(
            sym_rec.kind,
            SymbolKind::Struct | SymbolKind::Enum
        ) && callers.is_empty()
        {
            self.capture_impl_block_suggestions(name)
        } else {
            Vec::new()
        };

        ContextBundleView::Found(Box::new(ContextBundleFoundView {
            file_path: file.relative_path.clone(),
            body,
            kind_label: sym_rec.kind.to_string(),
            line_range: sym_rec.line_range,
            byte_count,
            callers: capture_section(&callers),
            callees: capture_callee_section(&callee_pairs),
            type_usages: capture_section(&type_usages),
            dependencies,
            implementation_suggestions,
        }))
    }

    /// Returns all `Call` references inside the given file whose
    /// `enclosing_symbol_index` equals `symbol_index`.
    ///
    /// These are the "callees" — functions called from within the target symbol.
    /// Consumed by `get_symbol_context` bundle mode (Plan 03).
    pub fn callees_for_symbol(
        &self,
        file_path: &str,
        symbol_index: usize,
    ) -> Vec<&ReferenceRecord> {
        match self.files.get(file_path) {
            None => vec![],
            Some(file) => {
                let symbol_range = file
                    .symbols
                    .get(symbol_index)
                    .map(|symbol| symbol.line_range);
                file.references
                    .iter()
                    .filter(|reference| {
                        if reference.kind != ReferenceKind::Call {
                            return false;
                        }

                        // Filter stdlib/iterator noise from callees (same filter as find_references).
                        if is_filtered_name(&reference.name, &file.language) {
                            return false;
                        }

                        if let Some((start_line, end_line)) = symbol_range {
                            reference.line_range.0 >= start_line
                                && reference.line_range.1 <= end_line
                        } else {
                            reference.enclosing_symbol_index == Some(symbol_index as u32)
                        }
                    })
                    .collect()
            }
        }
    }

    /// Returns all `TypeUsage` references inside the given symbol's line range.
    pub fn type_refs_for_symbol(
        &self,
        file_path: &str,
        symbol_index: usize,
    ) -> Vec<&ReferenceRecord> {
        match self.files.get(file_path) {
            None => vec![],
            Some(file) => {
                let symbol_range = file
                    .symbols
                    .get(symbol_index)
                    .map(|symbol| symbol.line_range);
                file.references
                    .iter()
                    .filter(|reference| {
                        if reference.kind != ReferenceKind::TypeUsage {
                            return false;
                        }
                        if let Some((start_line, end_line)) = symbol_range {
                            reference.line_range.0 >= start_line
                                && reference.line_range.1 <= end_line
                        } else {
                            reference.enclosing_symbol_index == Some(symbol_index as u32)
                        }
                    })
                    .collect()
            }
        }
    }

    /// Resolve type names to their definitions across the index.
    ///
    /// Returns definitions for custom types found in the index, excluding
    /// built-in/primitive types. Recurses up to `max_depth` levels to include
    /// transitive type dependencies.
    pub fn resolve_type_dependencies(
        &self,
        type_names: &[&str],
        max_depth: u8,
    ) -> Vec<TypeDependencyView> {
        const TYPE_DEF_KINDS: &[SymbolKind] = &[
            SymbolKind::Struct,
            SymbolKind::Enum,
            SymbolKind::Type,
            SymbolKind::Interface,
            SymbolKind::Class,
            SymbolKind::Trait,
        ];
        const MAX_DEPENDENCIES: usize = 15;

        let mut resolved: Vec<TypeDependencyView> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        let mut queue: Vec<(String, u8)> =
            type_names.iter().map(|n| (n.to_string(), 0u8)).collect();

        while let Some((name, depth)) = queue.pop() {
            if seen.contains(&name) || resolved.len() >= MAX_DEPENDENCIES {
                continue;
            }
            seen.insert(name.clone());

            // Search all files for a matching type definition.
            let mut found = false;
            for file in self.files.values() {
                for sym in &file.symbols {
                    if sym.name == name && TYPE_DEF_KINDS.contains(&sym.kind) && sym.depth == 0 {
                        let start = sym.byte_range.0 as usize;
                        let end = sym.byte_range.1 as usize;
                        let body = if end <= file.content.len() {
                            String::from_utf8_lossy(&file.content[start..end]).into_owned()
                        } else {
                            continue;
                        };

                        // If recursion budget remains, extract type refs from this definition.
                        if depth < max_depth {
                            for reference in &file.references {
                                if reference.kind == ReferenceKind::TypeUsage
                                    && reference.line_range.0 >= sym.line_range.0
                                    && reference.line_range.1 <= sym.line_range.1
                                    && !is_filtered_name(&reference.name, &file.language)
                                    && !seen.contains(&reference.name)
                                {
                                    queue.push((reference.name.clone(), depth + 1));
                                }
                            }
                        }

                        resolved.push(TypeDependencyView {
                            name: name.clone(),
                            kind_label: sym.kind.to_string(),
                            file_path: file.relative_path.clone(),
                            line_range: sym.line_range,
                            body,
                            depth,
                        });
                        found = true;
                        break;
                    }
                }
                if found {
                    break;
                }
            }
        }

        resolved.sort_by(|a, b| a.depth.cmp(&b.depth).then(a.name.cmp(&b.name)));
        resolved
    }

    fn capture_impl_block_suggestions(&self, type_name: &str) -> Vec<ImplBlockSuggestionView> {
        let inherent_name = format!("impl {type_name}");
        let trait_suffix = format!(" for {type_name}");
        let mut suggestions = Vec::new();

        for file in self.files.values() {
            for symbol in &file.symbols {
                if symbol.kind != SymbolKind::Impl {
                    continue;
                }
                let matches = symbol.name == inherent_name || symbol.name.ends_with(&trait_suffix);
                if !matches {
                    continue;
                }
                suggestions.push(ImplBlockSuggestionView {
                    display_name: symbol.name.clone(),
                    file_path: file.relative_path.clone(),
                    line_number: symbol.line_range.0 + 1,
                });
            }
        }

        suggestions.sort_by(|a, b| {
            a.file_path
                .cmp(&b.file_path)
                .then(a.line_number.cmp(&b.line_number))
                .then(a.display_name.cmp(&b.display_name))
        });
        suggestions
    }
}
