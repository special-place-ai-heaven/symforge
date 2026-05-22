use crate::domain::{LanguageId, ReferenceKind, ReferenceRecord, SymbolKind, SymbolRecord};

use super::store::IndexedFile;
pub(super) fn parse_reference_kind_filter(kind_filter: Option<&str>) -> Option<ReferenceKind> {
    match kind_filter {
        Some("call") => Some(ReferenceKind::Call),
        Some("import") => Some(ReferenceKind::Import),
        Some("type_usage") => Some(ReferenceKind::TypeUsage),
        Some("macro_use") => Some(ReferenceKind::MacroUse),
        Some("all") | None => None,
        _ => None,
    }
}

pub(super) fn matches_exact_symbol_qualified_name(
    language: &LanguageId,
    qualified_name: &str,
    target_name: &str,
    module_path: Option<&str>,
) -> bool {
    let separator = match language {
        LanguageId::Python => ".",
        LanguageId::JavaScript | LanguageId::TypeScript => "/",
        _ => "::",
    };

    let Some(module_path) = module_path else {
        return false;
    };

    let full_path = format!("{module_path}{separator}{target_name}");

    // Exact match: qualified_name == "crate::engine::optimize_deterministic"
    if qualified_name == full_path {
        return true;
    }

    // Suffix match: qualified_name "engine::optimize_deterministic" is a suffix of
    // "crate::engine::optimize_deterministic".  This handles qualified calls where
    // tree-sitter captures a partial path (no crate:: prefix).
    if full_path.ends_with(qualified_name)
        && full_path[..full_path.len() - qualified_name.len()].ends_with(separator)
    {
        return true;
    }

    false
}

pub(super) fn matches_exact_symbol_reference(
    reference: &ReferenceRecord,
    target_name: &str,
    target_language: &LanguageId,
    target_kind: SymbolKind,
    module_path: Option<&str>,
    reference_file: Option<&IndexedFile>,
    kind_filter: Option<ReferenceKind>,
) -> bool {
    if let Some(kind_filter) = kind_filter
        && reference.kind != kind_filter
    {
        return false;
    }

    if reference
        .qualified_name
        .as_deref()
        .is_some_and(|qualified_name| {
            matches_exact_symbol_qualified_name(
                target_language,
                qualified_name,
                target_name,
                module_path,
            )
        })
    {
        return true;
    }

    if reference.name != target_name {
        return false;
    }

    if target_kind == SymbolKind::Function && *target_language == LanguageId::Rust {
        if reference.qualified_name.is_some() {
            return false;
        }
        if is_rust_receiver_method_call(reference_file, reference) {
            return false;
        }
    }

    true
}

fn is_rust_receiver_method_call(
    reference_file: Option<&IndexedFile>,
    reference: &ReferenceRecord,
) -> bool {
    if reference.kind != ReferenceKind::Call {
        return false;
    }
    let Some(file) = reference_file else {
        return false;
    };
    if file.language != LanguageId::Rust {
        return false;
    }

    let start = reference.byte_range.0 as usize;
    start > 0 && file.content.get(start - 1) == Some(&b'.')
}

pub(crate) enum SymbolSelectorMatch<'a> {
    Selected(usize, &'a SymbolRecord),
    NotFound,
    Ambiguous(Vec<u32>),
}

/// Returns a disambiguation tier for a SymbolKind.  Lower number = higher
/// priority.  Used by `resolve_symbol_selector` to auto-pick the most
/// likely intended symbol when multiple candidates share the same name but
/// differ in kind.
///
/// Tier 1 — type definitions (class, struct, enum, interface, trait)
/// Tier 2 — namespace containers (module)
/// Tier 3 — callables (function, method, impl)
/// Tier 4 — everything else (constant, variable, type alias, key, section, other)
pub(super) fn kind_disambiguation_tier(kind: &SymbolKind) -> u8 {
    match kind {
        SymbolKind::Class
        | SymbolKind::Struct
        | SymbolKind::Enum
        | SymbolKind::Interface
        | SymbolKind::Trait => 1,
        SymbolKind::Module => 2,
        SymbolKind::Function | SymbolKind::Method | SymbolKind::Impl => 3,
        _ => 4, // Constant, Variable, Type, Key, Section, Other
    }
}

pub(crate) fn resolve_symbol_selector<'a>(
    file: &'a IndexedFile,
    name: &str,
    symbol_kind: Option<&str>,
    symbol_line: Option<u32>,
) -> SymbolSelectorMatch<'a> {
    let candidates = find_candidates_cascade(file, name, symbol_kind, symbol_line);

    match candidates.len() {
        0 => SymbolSelectorMatch::NotFound,
        1 => {
            let (idx, symbol) = candidates[0];
            SymbolSelectorMatch::Selected(idx, symbol)
        }
        _ => {
            // Kind-tier disambiguation: when the user did NOT specify
            // symbol_kind, group candidates by kind tier and auto-select
            // if the highest tier has exactly one candidate.  This resolves
            // the common C#/Java/Kotlin pattern where a class and its
            // constructor share the same name (class=tier 1, constructor
            // mapped to Function=tier 3).
            if symbol_kind.is_none() {
                let min_tier = candidates
                    .iter()
                    .map(|(_, sym)| kind_disambiguation_tier(&sym.kind))
                    .min()
                    .unwrap(); // safe: candidates.len() >= 2

                let top_tier: Vec<usize> = candidates
                    .iter()
                    .enumerate()
                    .filter(|(_, (_, sym))| kind_disambiguation_tier(&sym.kind) == min_tier)
                    .map(|(i, _)| i)
                    .collect();

                if top_tier.len() == 1 {
                    let (idx, symbol) = candidates[top_tier[0]];
                    return SymbolSelectorMatch::Selected(idx, symbol);
                }
            }

            SymbolSelectorMatch::Ambiguous(
                candidates
                    .into_iter()
                    .map(|(_, symbol)| symbol.line_range.0)
                    .collect(),
            )
        }
    }
}

// ---------------------------------------------------------------------------
// Cascading name resolution
// ---------------------------------------------------------------------------

/// Try progressively fuzzier name-matching strategies, returning candidates
/// from the first strategy that produces any matches.  Ordered from most
/// specific to least specific to minimise false-positive risk.
///
/// 1. True exact match (no allocation, covers ~99% of lookups)
/// 2. Whitespace-normalised match (Rust generics, CSS spacing)
/// 3. `impl` prefix prepend (Rust: LLMs drop "impl " from impl block names)
/// 4. Qualification stripping (C++ `Foo::bar` → `bar`, Go `Type.Method` → `Method`)
/// 5. CSS/SCSS selector prefix (LLM sends `@media`, index has `@media (max-width: …)`)
/// 6. Swift extension prefix (LLM sends `extension MyClass: Drawable`, index has `MyClass`)
fn find_candidates_cascade<'a>(
    file: &'a IndexedFile,
    name: &str,
    symbol_kind: Option<&str>,
    symbol_line: Option<u32>,
) -> Vec<(usize, &'a SymbolRecord)> {
    // 1. True exact match — fast path, no allocation.
    let c = match_symbols(file, |s| s.name == name, symbol_kind, symbol_line);
    if !c.is_empty() {
        return c;
    }

    let norm_query = normalize_symbol_name(name);

    // 2. Whitespace-normalised match (handles Rust generics, CSS spacing).
    //    Only try when normalisation actually changed the string.
    if norm_query != name {
        let c = match_symbols(
            file,
            |s| normalize_symbol_name(&s.name) == norm_query,
            symbol_kind,
            symbol_line,
        );
        if !c.is_empty() {
            return c;
        }
    }

    // 3. Impl prefix: "Trait for Type" → "impl Trait for Type"
    //    Also handles the combined case of missing prefix + whitespace diff.
    let impl_query = format!("impl {norm_query}");
    let c = match_symbols(
        file,
        |s| normalize_symbol_name(&s.name) == impl_query,
        symbol_kind,
        symbol_line,
    );
    if !c.is_empty() {
        return c;
    }

    // 4. Qualification stripping: "Foo::bar" → "bar", "Type.Method" → "Method"
    //    Covers C++ qualified method definitions and Go receiver methods.
    if let Some(bare) = strip_qualification(name) {
        let c = match_symbols(file, |s| s.name == bare, symbol_kind, symbol_line);
        if !c.is_empty() {
            return c;
        }
    }

    // 5. CSS/SCSS selector/at-rule prefix matching.
    //    "@media" matches "@media (max-width: 768px)"; ".btn" matches ".btn, .btn-primary".
    //    Only for names starting with CSS-like prefixes to avoid false positives.
    if name.starts_with('@') || name.starts_with('.') || name.starts_with('#') {
        let c = match_symbols(
            file,
            |s| is_selector_prefix_match(&s.name, name),
            symbol_kind,
            symbol_line,
        );
        if !c.is_empty() {
            return c;
        }
    }

    // 6. Swift extension: "extension MyClass: Drawable" → "MyClass"
    if let Some(type_name) = strip_swift_extension(name) {
        let c = match_symbols(file, |s| s.name == type_name, symbol_kind, symbol_line);
        if !c.is_empty() {
            return c;
        }
    }

    Vec::new()
}

/// Collect symbols matching a name predicate, kind filter, and optional line filter.
fn match_symbols<'a, F>(
    file: &'a IndexedFile,
    name_pred: F,
    symbol_kind: Option<&str>,
    symbol_line: Option<u32>,
) -> Vec<(usize, &'a SymbolRecord)>
where
    F: Fn(&SymbolRecord) -> bool,
{
    let mut candidates: Vec<(usize, &SymbolRecord)> = file
        .symbols
        .iter()
        .enumerate()
        .filter(|(_, s)| {
            name_pred(s)
                && symbol_kind
                    .map(|k| s.kind.to_string().eq_ignore_ascii_case(k))
                    .unwrap_or(true)
        })
        .collect();
    if let Some(sl) = symbol_line {
        // symbol_line is 1-based (from search_symbols output); line_range is 0-based.
        candidates.retain(|(_, s)| s.line_range.0 + 1 == sl);
    }
    candidates
}

/// Normalise a symbol name for fuzzy comparison.
/// Collapses whitespace runs to single spaces and strips spaces around
/// generic brackets and commas so `"Vec< T >"` matches `"Vec<T>"` and
/// `"HashMap< K , V >"` matches `"HashMap<K,V>"`.
fn normalize_symbol_name(s: &str) -> String {
    let collapsed: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed
        .replace("< ", "<")
        .replace(" >", ">")
        .replace(" ,", ",")
        .replace(", ", ",")
}

/// Strip namespace/type qualification.
/// `"Foo::bar"` → `Some("bar")`, `"Type.Method"` → `Some("Method")`, `"plain"` → `None`.
fn strip_qualification(name: &str) -> Option<&str> {
    name.rsplit_once("::")
        .map(|(_, bare)| bare)
        .or_else(|| name.rsplit_once('.').map(|(_, bare)| bare))
}

/// Check whether `stored` is a CSS selector or at-rule that starts with `query`
/// followed by a delimiter (comma, space, paren, brace) or end-of-string.
/// Prevents `".btn"` from matching `".btn-group"` while still matching
/// `".btn, .btn-primary"`.
fn is_selector_prefix_match(stored: &str, query: &str) -> bool {
    if !stored.starts_with(query) {
        return false;
    }
    stored.len() == query.len()
        || matches!(
            stored.as_bytes()[query.len()],
            b',' | b' ' | b'(' | b'{' | b'\t' | b'\n' | b')'
        )
}

/// Strip Swift `extension` prefix and optional protocol conformance.
/// `"extension MyClass: Drawable"` → `Some("MyClass")`,
/// `"extension MyClass"` → `Some("MyClass")`,
/// `"MyClass"` → `None`.
fn strip_swift_extension(name: &str) -> Option<&str> {
    let rest = name.strip_prefix("extension ")?;
    Some(rest.split(':').next().unwrap_or(rest).trim())
}

pub(crate) fn render_symbol_selector(
    name: &str,
    symbol_kind: Option<&str>,
    symbol_line: Option<u32>,
) -> String {
    match (symbol_kind, symbol_line) {
        (Some(symbol_kind), Some(symbol_line)) => {
            format!("{symbol_kind} {name} at line {symbol_line}")
        }
        (Some(symbol_kind), None) => format!("{symbol_kind} {name}"),
        (None, Some(symbol_line)) => format!("{name} at line {symbol_line}"),
        (None, None) => name.to_string(),
    }
}
