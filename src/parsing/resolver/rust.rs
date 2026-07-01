//! Rust same-file + `use`-import call resolution (SP-0C spike).

use std::collections::{HashMap, HashSet};

use super::{ResolvedCall, ResolverStrategy};
use crate::domain::{LanguageId, ReferenceKind, SymbolKind};

/// Resolve every `Call` reference in a single Rust source string.
///
/// Strategy priority, per resolver-port-notes.md: same-file definitions first,
/// then in-file `use` imports/aliases. Anything else (stdlib, trait dispatch,
/// cross-file, method on an inferred receiver) is left `Unresolved`.
pub fn resolve_rust_source(source: &str) -> Vec<ResolvedCall> {
    let result = crate::parsing::process_file("fixture.rs", source.as_bytes(), LanguageId::Rust);

    // Same-file callable definitions (free functions + methods).
    let local_callables: HashSet<&str> = result
        .symbols
        .iter()
        .filter(|s| matches!(s.kind, SymbolKind::Function | SymbolKind::Method))
        .map(|s| s.name.as_str())
        .collect();

    // Same-file type definitions (for `Type::assoc()` resolution).
    let local_types: HashSet<&str> = result
        .symbols
        .iter()
        .filter(|s| {
            matches!(
                s.kind,
                SymbolKind::Struct
                    | SymbolKind::Enum
                    | SymbolKind::Trait
                    | SymbolKind::Type
                    | SymbolKind::Class
            )
        })
        .map(|s| s.name.as_str())
        .collect();

    // In-file imports: imported simple name -> full path.
    let mut imports: HashMap<&str, String> = HashMap::new();
    for r in &result.references {
        if r.kind == ReferenceKind::Import {
            let full = r.qualified_name.clone().unwrap_or_else(|| r.name.clone());
            imports.insert(r.name.as_str(), full);
        }
    }

    // `use orig as alias` aliases: alias -> original simple name.
    let aliases = &result.alias_map;

    result
        .references
        .iter()
        .filter(|r| r.kind == ReferenceKind::Call)
        .map(|r| {
            let (strategy, callee_qname) =
                resolve_one(r, &local_callables, &local_types, &imports, aliases);
            ResolvedCall {
                name: r.name.clone(),
                line: r.line_range.0 + 1,
                callee_qname,
                strategy,
            }
        })
        .collect()
}

fn resolve_one(
    r: &crate::domain::ReferenceRecord,
    local_callables: &HashSet<&str>,
    local_types: &HashSet<&str>,
    imports: &HashMap<&str, String>,
    aliases: &HashMap<String, String>,
) -> (ResolverStrategy, Option<String>) {
    // Qualified call `Head::...::method()` — resolve via the head segment.
    if let Some(qualified) = &r.qualified_name
        && let Some((head, method)) = qualified.rsplit_once("::")
    {
        let head_first = head.split("::").next().unwrap_or(head);

        // Same-file: `LocalType::local_method()`.
        if local_types.contains(head_first) && local_callables.contains(method) {
            return (
                ResolverStrategy::SameFile,
                Some(format!("{head}::{method}")),
            );
        }
        // Import: `ImportedType::method()` -> imported_path::method.
        if let Some(path) = imports.get(head_first) {
            return (ResolverStrategy::Import, Some(format!("{path}::{method}")));
        }
        // Alias: `AliasedType::method()`.
        if let Some(orig) = aliases.get(head_first) {
            return (ResolverStrategy::Import, Some(format!("{orig}::{method}")));
        }
    }

    // Simple / method call by bare name.
    let name = r.name.as_str();
    if local_callables.contains(name) {
        return (ResolverStrategy::SameFile, Some(name.to_string()));
    }
    if let Some(path) = imports.get(name) {
        return (ResolverStrategy::Import, Some(path.clone()));
    }
    if let Some(orig) = aliases.get(name) {
        return (ResolverStrategy::Import, Some(orig.clone()));
    }

    (ResolverStrategy::Unresolved, None)
}
