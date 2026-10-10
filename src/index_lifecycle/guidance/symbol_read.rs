//! Pure symbol retrieval and inspection renderers shared by native and MCP.
use super::file_read::{not_found_file, render_not_found_symbol};
#[cfg(feature = "server")]
use crate::live_index::SymbolDetailView;
use crate::live_index::{IndexedFile, InspectMatchView, LiveIndex};

/// Return the full source body for a named symbol plus a footer.
///
/// Footer: `[{kind}, lines {start}-{end}, {byte_count} bytes]`
/// Not-found: see `not_found_symbol`
#[cfg(feature = "server")]
pub fn symbol_detail(
    index: &LiveIndex,
    path: &str,
    name: &str,
    kind_filter: Option<&str>,
) -> String {
    match index.capture_shared_file(path) {
        Some(file) => symbol_detail_from_indexed_file(file.as_ref(), name, kind_filter, None),
        None => not_found_file(path),
    }
}

pub fn symbol_detail_from_indexed_file(
    file: &IndexedFile,
    name: &str,
    kind_filter: Option<&str>,
    symbol_line: Option<u32>,
) -> String {
    use crate::live_index::query::{SymbolSelectorMatch, resolve_symbol_selector};

    match resolve_symbol_selector(file, name, kind_filter, symbol_line) {
        SymbolSelectorMatch::Selected(_idx, sym) => {
            let start = sym.effective_start() as usize;
            let end = sym.byte_range.1 as usize;
            let clamped_end = end.min(file.content.len());
            let clamped_start = start.min(clamped_end);
            let body =
                String::from_utf8_lossy(&file.content[clamped_start..clamped_end]).into_owned();
            let byte_count = end.saturating_sub(start);
            format!(
                "{}\n[{}, lines {}-{}, {} bytes]",
                body,
                sym.kind,
                sym.line_range.0 + 1,
                sym.line_range.1 + 1,
                byte_count
            )
        }
        SymbolSelectorMatch::NotFound => {
            // D18: loud not-found. Do NOT silently substitute a content-anchored
            // window (a substring hit in comments/strings) for the symbol the
            // caller asked for. render_not_found_symbol names the symbol + path,
            // offers near-miss suggestions, and (via "No symbol ") auto-classifies
            // as OutcomeClass::NotFound in the result envelope.
            render_not_found_symbol(&file.relative_path, &file.symbols, name)
        }
        SymbolSelectorMatch::Ambiguous(lines) => {
            let line_strs: Vec<String> = lines.iter().map(|l| format!("{}", l + 1)).collect();
            format!(
                "Ambiguous: {} `{}` symbols in {} (lines {}). Pass symbol_line to disambiguate.",
                lines.len(),
                name,
                file.relative_path,
                line_strs.join(", ")
            )
        }
    }
}

/// Compatibility renderer for `SymbolDetailView`.
///
/// Main hot-path readers should prefer `symbol_detail_from_indexed_file()`.
#[cfg(feature = "server")]
pub fn symbol_detail_view(
    view: &SymbolDetailView,
    name: &str,
    kind_filter: Option<&str>,
) -> String {
    render_symbol_detail(
        &view.relative_path,
        &view.content,
        &view.symbols,
        name,
        kind_filter,
    )
}

#[cfg(feature = "server")]
fn render_symbol_detail(
    relative_path: &str,
    content: &[u8],
    symbols: &[crate::domain::SymbolRecord],
    name: &str,
    kind_filter: Option<&str>,
) -> String {
    let matching: Vec<&crate::domain::SymbolRecord> = symbols
        .iter()
        .filter(|s| {
            s.name == name
                && kind_filter
                    .map(|k| s.kind.to_string().eq_ignore_ascii_case(k))
                    .unwrap_or(true)
        })
        .collect();

    match matching.first() {
        None => render_not_found_symbol(relative_path, symbols, name),
        Some(s) => {
            let start = s.effective_start() as usize;
            let end = s.byte_range.1 as usize;
            let clamped_end = end.min(content.len());
            let clamped_start = start.min(clamped_end);
            let body = String::from_utf8_lossy(&content[clamped_start..clamped_end]).into_owned();
            let byte_count = end.saturating_sub(start);
            let mut result = format!(
                "{}\n[{}, lines {}-{}, {} bytes]",
                body,
                s.kind,
                s.line_range.0 + 1,
                s.line_range.1 + 1,
                byte_count
            );
            if matching.len() > 1 {
                let others = matching.len() - 1;
                let lines: Vec<String> = matching[1..]
                    .iter()
                    .map(|m| format!("{}", m.line_range.0 + 1))
                    .collect();
                result.push_str(&format!(
                    "\nNote: {} more `{}` in this file (line {}). Use symbol_line to disambiguate.",
                    others,
                    name,
                    lines.join(", ")
                ));
            }
            result
        }
    }
}

pub fn code_slice_view(path: &str, slice: &[u8]) -> String {
    let text = String::from_utf8_lossy(slice).into_owned();
    format!("{path}\n{text}")
}

#[cfg(feature = "server")]
pub fn code_slice_from_indexed_file(
    file: &IndexedFile,
    start_byte: usize,
    end_byte: Option<usize>,
) -> String {
    let end = end_byte
        .unwrap_or(file.content.len())
        .min(file.content.len());
    let start = start_byte.min(end);
    // Safety: `file.content` is `Vec<u8>`, so this is a BYTE-slice index (no
    // char-boundary panic), and the clamps above guarantee 0 <= start <= end <=
    // len (no out-of-range panic) even for untrusted caller-supplied byte offsets.
    // `code_slice_view` decodes via `String::from_utf8_lossy`. Keep `content` as
    // `Vec<u8>` (not `String`) to preserve this no-panic invariant.
    code_slice_view(&file.relative_path, &file.content[start..end])
}

/// Format results of `inspect_match`.
pub fn inspect_match_result_view(view: &InspectMatchView) -> String {
    match view {
        InspectMatchView::FileNotFound { path } => not_found_file(path),
        InspectMatchView::LineOutOfBounds {
            path,
            line,
            total_lines,
        } => {
            format!("Line {line} is out of bounds for {path} (file has {total_lines} lines).")
        }
        InspectMatchView::Found(found) => {
            let mut output = String::new();

            // 1. Excerpt
            output.push_str(&found.excerpt);
            output.push('\n');

            // 2. Parent chain (shows full nesting context when deeper than 1 level)
            if found.parent_chain.len() > 1 {
                output.push_str("\nScope: ");
                let chain: Vec<String> = found
                    .parent_chain
                    .iter()
                    .map(|p| symbol_kind_name_label(&p.kind_label, &p.name))
                    .collect();
                output.push_str(&chain.join(" → "));
            }

            // 3. Enclosing symbol (deepest)
            if let Some(enclosing) = &found.enclosing {
                output.push_str(&format_enclosing(enclosing));
            } else {
                output.push_str("\n(No enclosing symbol)");
            }

            // 4. Siblings
            if !found.siblings.is_empty() || found.siblings_overflow > 0 {
                output.push_str(&format_siblings(&found.siblings, found.siblings_overflow));
            }

            output
        }
    }
}

/// Join a symbol kind label and name without doubling the kind token when the
/// name already begins with it. Some symbols (notably `impl`/`trait` blocks)
/// store a display name that already carries the kind prefix, e.g. the `impl`
/// block for `LanguageId` is stored as name "impl LanguageId" with kind "impl".
/// Naively prepending the kind label then yields "impl impl LanguageId". When
/// the name already starts with the kind token followed by whitespace (or equals
/// it exactly), this returns the name unchanged; otherwise it prepends the kind.
pub(crate) fn symbol_kind_name_label(kind_label: &str, name: &str) -> String {
    if kind_label.is_empty() {
        return name.to_string();
    }
    let trimmed = name.trim_start();
    let already_prefixed = trimmed == kind_label
        || trimmed
            .strip_prefix(kind_label)
            .is_some_and(|rest| rest.starts_with(char::is_whitespace));
    if already_prefixed {
        name.to_string()
    } else {
        format!("{kind_label} {name}")
    }
}

fn format_enclosing(enclosing: &crate::live_index::EnclosingSymbolView) -> String {
    format!(
        "\nEnclosing symbol: {} (lines {}-{})",
        symbol_kind_name_label(&enclosing.kind_label, &enclosing.name),
        enclosing.line_range.0,
        enclosing.line_range.1
    )
}

pub(crate) fn format_siblings(
    siblings: &[crate::live_index::SiblingSymbolView],
    overflow: usize,
) -> String {
    let mut lines = vec!["\nNearby siblings:".to_string()];
    for sib in siblings {
        lines.push(format!(
            "  {:<12} {:<30} {}-{}",
            sib.kind_label, sib.name, sib.line_range.0, sib.line_range.1
        ));
    }
    if overflow > 0 {
        lines.push(format!("  ... and {overflow} more siblings"));
    }
    lines.join("\n")
}

/// Outcome of a name-only `get_symbol` lookup (Wave 1 Fix 2).
pub(crate) enum SymbolNameLookup {
    /// Exactly one match — path and canonical indexed symbol name.
    Unique { path: String, symbol_name: String },
    /// More than one exact-name match — a ready-to-return disambiguation listing.
    Ambiguous(String, Vec<crate::live_index::search::SymbolSearchHit>),
    /// No exact-name match — a ready-to-return loud not-found (D18 style).
    NotFound(String),
}

/// Render the disambiguation listing for a name-only `get_symbol` hit that
/// matched multiple symbols. Shows path + start line + kind per candidate,
/// capped at 20 with a count disclosure, and names how to disambiguate.
fn render_symbol_name_ambiguity(
    name: &str,
    hits: &[crate::live_index::search::SymbolSearchHit],
    overflowed: bool,
) -> String {
    const MAX_CANDIDATES: usize = 20;
    // Under the 500-hit search cap the exact count can under-report; when the
    // search overflowed, disclose the total as a lower bound (Wave 1 Fix 4).
    let total = if overflowed {
        format!("more than {}", hits.len())
    } else {
        hits.len().to_string()
    };
    let mut output = format!(
        "Ambiguous symbol selector: {total} symbols named \"{name}\" found across the index.\n\
         Pass `path` (and `symbol_line` if a file has several) to disambiguate.\n\
         Candidates:",
    );
    for hit in hits.iter().take(MAX_CANDIDATES) {
        output.push_str(&format!(
            "\n- {} (line {}, {})",
            hit.path, hit.line, hit.kind
        ));
    }
    if hits.len() > MAX_CANDIDATES {
        output.push_str(&format!(
            "\n... and {} more (use search_symbols to list them all)",
            hits.len() - MAX_CANDIDATES
        ));
    }
    output
}

pub(crate) fn resolve_symbol_path_by_name(
    index: &LiveIndex,
    name: &str,
    kind: Option<&str>,
    symbol_line: Option<u32>,
) -> SymbolNameLookup {
    use crate::live_index::search::{
        ResultLimit, SymbolMatchTier, SymbolSearchOptions, search_symbols_with_options,
    };
    // Permissive noise policy (the default) so a symbol defined only in a
    // test/vendor/generated file is still resolvable by exact name — an
    // explicit path already reaches those files. A generous limit keeps the
    // ambiguity count honest without changing the O(symbols) scan cost.
    let options = SymbolSearchOptions {
        result_limit: ResultLimit::new(500),
        ..Default::default()
    };
    let result = search_symbols_with_options(index, name, kind, &options);
    // Under the 500-hit search cap, `overflow_count > 0` means more matches
    // exist than were returned, so the ambiguity total must be a lower bound,
    // never an exact under-report (Wave 1 Fix 4).
    let overflowed = result.overflow_count > 0;
    let hits = result.hits;
    // Exact tier + exact (case-sensitive) name only — a name selector must
    // not silently resolve to a prefix/substring neighbor.
    let mut exact: Vec<crate::live_index::search::SymbolSearchHit> = hits
        .iter()
        .filter(|hit| hit.tier == SymbolMatchTier::Exact && hit.name == name)
        .cloned()
        .collect();
    // `symbol_line` narrows to matches at that 1-based start line when it
    // resolves at least one candidate (e.g. two files, one line each).
    if let Some(line) = symbol_line {
        let narrowed: Vec<_> = exact
            .iter()
            .filter(|hit| hit.line == line)
            .cloned()
            .collect();
        if !narrowed.is_empty() {
            exact = narrowed;
        }
    }
    match exact.len() {
        0 => {
            // Kind-mismatch guard (Wave 1 Fix 1): before claiming the name is
            // absent, re-check WITHOUT the kind filter. If it exists under other
            // kinds, "No symbol named X" is factually false — name the kinds it
            // DOES have so the caller can drop or correct the filter.
            if let Some(requested_kind) = kind {
                let unfiltered = search_symbols_with_options(index, name, None, &options);
                let mut kinds: Vec<String> = unfiltered
                    .hits
                    .into_iter()
                    .filter(|hit| hit.tier == SymbolMatchTier::Exact && hit.name == name)
                    .map(|hit| hit.kind)
                    .collect();
                kinds.sort();
                kinds.dedup();
                if !kinds.is_empty() {
                    return SymbolNameLookup::NotFound(format!(
                        "`{name}` exists in the index, but not as kind={requested_kind} \
                         (found: {}). Drop the kind filter or pass the correct kind.",
                        kinds.join(", ")
                    ));
                }
            }
            // Unique snake_case prefix (`reconcile` → `reconcile_orders`) when the
            // query names a single indexed symbol unambiguously.
            if kind.is_none() {
                let prefix: Vec<_> = hits
                    .iter()
                    .filter(|hit| {
                        hit.tier == SymbolMatchTier::Prefix
                            && hit.name.starts_with(name)
                            && hit.name.as_bytes().get(name.len()) == Some(&b'_')
                    })
                    .cloned()
                    .collect();
                if prefix.len() == 1 {
                    return SymbolNameLookup::Unique {
                        path: prefix[0].path.clone(),
                        symbol_name: prefix[0].name.clone(),
                    };
                }
            }
            SymbolNameLookup::NotFound(format!(
                "No symbol named `{name}` in the index. \
                 Use search_symbols(query=\"{name}\") for a fuzzy lookup, \
                 or pass an explicit `path` if the file is not indexed."
            ))
        }
        1 => SymbolNameLookup::Unique {
            path: exact[0].path.clone(),
            symbol_name: exact[0].name.clone(),
        },
        _ => SymbolNameLookup::Ambiguous(
            render_symbol_name_ambiguity(name, &exact, overflowed),
            exact,
        ),
    }
}
