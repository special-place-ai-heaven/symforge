//! Pure formatting functions for all 10 tool responses.
//!
//! All functions take `&LiveIndex` (or data derived from it) and return `String`.
//! No I/O, no async. Output matches the locked formats defined in CONTEXT.md.

// Feature 020 V11 claim attribution (T043). The frozen contract places the file
// at `src/protocol/claim_provenance.rs`, and that is where it lives. The
// DECLARATION is here, in `format.rs`, and deliberately NOT in
// `src/protocol/mod.rs`:
//
//   * `protocol/mod.rs` is inside the `publication_roots` set of the frozen
//     retirement census, so adding a `mod` line there is a release-compiled edit
//     of a hashed file and would move that digest. `format.rs` is in none of the
//     five closure path lists.
//   * `protocol/mod.rs` declares `read_gate` as `pub(crate) mod`, so anchoring
//     there would make the whole provenance module crate-private — and
//     `tests/claim_provenance_v11.rs` is a SEPARATE CRATE that could never see
//     it. `format.rs` is `pub mod`, so the types are reachable from the oracles.
//
// `#[path]` resolves relative to the directory CONTAINING this file, so the bare
// name lands on `src/protocol/claim_provenance.rs`. Verified against rustc
// rather than recalled: `#[path = "../claim_provenance.rs"]` makes it look for
// `src/claim_provenance.rs` and fail.
//
// Do NOT "tidy" this into `protocol/mod.rs`. It silently moves a frozen digest
// and breaks the oracles' visibility in one edit.
#[path = "claim_provenance.rs"]
pub mod claim_provenance;

pub use crate::index_lifecycle::guidance::health::*;
pub use crate::index_lifecycle::guidance::reference_read::OutputLimits;

use crate::domain::index::{AdmissionTier, SkipReason, SkippedFile};
#[cfg(test)]
use crate::domain::{CoverageStatus, FileDisposition, FreshnessStatus, RepositoryManifest};
#[cfg(test)]
use crate::live_index::{
    ContextBundleFoundView, ContextBundleReferenceView, ContextBundleSectionView,
    ContextBundleView, FindReferencesView, HealthStats, ImplBlockSuggestionView, IndexLoadSource,
    TypeDependencyView,
};
use crate::live_index::{
    FileOutlineView, IndexedFile, LiveIndex, RepoOutlineFileView, RepoOutlineView,
    SnapshotVerifyState, search,
};
use crate::protocol::surface_probe::{SurfaceProfile, connection_surface_or_env};
use crate::{cli::hook::HookAdoptionSnapshot, sidecar::StatsSnapshot};

pub fn capability_evidence_line(evidence: &crate::capability::CapabilityEvidence) -> String {
    let mut line = format!("Capability: {} {}", evidence.capability, evidence.status);
    if let Some(detail) = evidence.detail.as_deref().map(str::trim)
        && !detail.is_empty()
    {
        let detail = detail.trim_end_matches('.');
        line.push_str(" - ");
        line.push_str(detail);
    }
    line.push('.');
    line
}

/// Format the file outline for a given path.
///
/// Header: `{path}  ({N} symbols)`
/// Body: each symbol indented by `depth * 2` spaces, then `{kind:<12} {name:<30} {start}-{end}`
/// Not-found: "File not found: {path}"
pub fn file_outline(index: &LiveIndex, path: &str) -> String {
    match index.capture_shared_file(path) {
        Some(file) => file_outline_from_indexed_file(file.as_ref()),
        None => not_found_file(path),
    }
}

pub fn file_outline_from_indexed_file(file: &IndexedFile) -> String {
    render_file_outline(&file.relative_path, &file.symbols)
}

fn render_file_outline(relative_path: &str, symbols: &[crate::domain::SymbolRecord]) -> String {
    let mut lines = Vec::new();
    lines.push(format!("{}  ({} symbols)", relative_path, symbols.len()));

    for sym in symbols {
        let indent = "  ".repeat(sym.depth as usize);
        let kind_str = sym.kind.to_string();
        lines.push(format!(
            "{}{:<12} {:<30} {}-{}",
            indent,
            kind_str,
            sym.name,
            sym.line_range.0 + 1,
            sym.line_range.1 + 1
        ));
    }

    lines.join("\n")
}

pub fn collapse_large_test_modules(
    text: String,
    include_tests: bool,
    sections: Option<&[String]>,
) -> String {
    crate::index_lifecycle::guidance::read_context::collapse_large_test_modules(
        text,
        include_tests,
        sections,
    )
}
/// Compatibility renderer for `FileOutlineView`.
///
/// Main hot-path readers should prefer `file_outline_from_indexed_file()`.
pub fn file_outline_view(view: &FileOutlineView) -> String {
    render_file_outline(&view.relative_path, &view.symbols)
}

pub use crate::index_lifecycle::guidance::symbol_read::inspect_match_result_view;
#[cfg(test)]
use crate::index_lifecycle::guidance::symbol_read::symbol_kind_name_label;
pub use crate::index_lifecycle::guidance::symbol_read::{
    code_slice_from_indexed_file, code_slice_view, symbol_detail, symbol_detail_from_indexed_file,
    symbol_detail_view,
};

/// Search for symbols matching a query (case-insensitive), with 3-tier scored ranking.
///
/// Output sections (only non-empty tiers shown):
/// ```text
/// ── Exact matches ──
///   {line}: {kind} {name}  ({file})
///
/// ── Prefix matches ──
///   ...
///
/// ── Substring matches ──
///   ...
/// ```
/// Header: `{N} matches in {M} files`
/// Empty: "No symbols matching '{query}'"
pub fn search_symbols_result(index: &LiveIndex, query: &str) -> String {
    search_symbols_result_with_kind(index, query, None)
}

pub fn search_symbols_result_with_kind(
    index: &LiveIndex,
    query: &str,
    kind_filter: Option<&str>,
) -> String {
    let result = search::search_symbols(
        index,
        query,
        kind_filter,
        search::ResultLimit::symbol_search_default().get(),
    );
    search_symbols_result_view(&result, query)
}

pub use crate::index_lifecycle::guidance::search_render::search_symbols_result_view;

/// Search for text content matches (case-insensitive substring).
///
/// For queries >= 3 chars, uses the TrigramIndex to select candidate files before scanning.
/// For queries < 3 chars, falls back to scanning all files (trigram search handles this internally).
///
/// Header: `{N} matches in {M} files`
/// Body: grouped by file, each match: `  {line_number}: {line_content}`
/// Empty: "No matches for '{query}'"
pub fn search_text_result(index: &LiveIndex, query: &str) -> String {
    search_text_result_with_options(index, Some(query), None, false)
}

pub fn search_text_result_with_options(
    index: &LiveIndex,
    query: Option<&str>,
    terms: Option<&[String]>,
    regex: bool,
) -> String {
    let result = search::search_text(index, query, terms, regex);
    search_text_result_view(
        result,
        None,
        None,
        None,
        SearchSuggestionContext {
            regex,
            include_tests: false,
            multi_word_literal: !regex
                && query.is_some_and(|q| q.trim().contains(char::is_whitespace)),
        },
    )
}

pub(crate) use crate::index_lifecycle::guidance::search_render::append_excluded_knowledge_note;
pub use crate::index_lifecycle::guidance::search_render::{
    SearchSuggestionContext, search_text_result_view,
};

/// Generate a depth-limited source file tree with symbol counts per file and directory.
///
/// - `path`: subtree prefix filter (empty/blank = project root).
/// - `depth`: maximum depth levels to expand (default 2, max 5).
///
/// Output format:
/// ```text
/// {dir}/  ({N} files, {M} symbols)
///   {file} [{lang}]  ({K} symbols)
///   {subdir}/  ({N} files, {M} symbols)
/// ...
/// {D} directories, {F} files, {S} symbols
/// ```
pub fn file_tree(index: &LiveIndex, path: &str, depth: u32) -> String {
    let view = index.capture_repo_outline_view();
    file_tree_view(&view.files, path, depth)
}

pub fn file_tree_view(files: &[RepoOutlineFileView], path: &str, depth: u32) -> String {
    crate::index_lifecycle::guidance::read_context::file_tree_view(files, path, depth)
}

pub fn file_tree_view_with_skipped(
    files: &[RepoOutlineFileView],
    skipped: &[SkippedFile],
    path: &str,
    depth: u32,
) -> String {
    crate::index_lifecycle::guidance::read_context::file_tree_view_with_skipped(
        files, skipped, path, depth,
    )
}
/// Generate a directory-tree overview of the repo.
///
/// Header: `{project_name}  ({N} files, {M} symbols)`
/// Body: sorted paths, each: `  {filename:<20} {language:<12} {symbol_count} symbols`
pub fn repo_outline(index: &LiveIndex, project_name: &str) -> String {
    let view = index.capture_repo_outline_view();
    repo_outline_view(&view, project_name)
}

pub fn repo_outline_view(view: &RepoOutlineView, project_name: &str) -> String {
    crate::index_lifecycle::guidance::read_context::repo_outline_view(view, project_name)
}
/// Generate a health report for the index.
///
/// Watcher state is read from `health_stats()` (Off defaults when no watcher is active).
/// Use `health_report_with_watcher` when the live `WatcherInfo` should be reflected.
///
/// Format:
/// ```text
/// Status: {Ready|Empty|Degraded}
/// Files:  {N} indexed ({P} parsed, {PP} partial, {F} failed)
/// Symbols: {S}
/// Loaded in: {D}ms
/// Watcher: active ({E} events, last: {T}, debounce: {D}ms)
///     or: degraded ({E} events processed before failure)
///     or: off
/// ```
pub fn health_report(index: &LiveIndex) -> String {
    use crate::live_index::IndexState;

    let state = index.index_state();
    let status = match state {
        IndexState::Empty => "Empty",
        IndexState::Ready => "Ready",
        IndexState::Loading => "Loading",
        IndexState::CircuitBreakerTripped { .. } => "Degraded",
    };
    let stats = index.health_stats();
    health_report_from_stats(status, &stats, 0)
}

/// Generate a health report for the index with live watcher state.
///
/// Uses `health_stats_with_watcher` to incorporate the live `WatcherInfo` into the report.
/// Called by the `health` tool handler in production (watcher is always available there).
pub fn health_report_with_watcher(
    index: &LiveIndex,
    watcher: &crate::watcher::WatcherInfo,
) -> String {
    use crate::live_index::IndexState;

    let state = index.index_state();
    let status = match state {
        IndexState::Empty => "Empty",
        IndexState::Ready => "Ready",
        IndexState::Loading => "Loading",
        IndexState::CircuitBreakerTripped { .. } => "Degraded",
    };
    let stats = index.health_stats_with_watcher(watcher);
    health_report_from_stats(status, &stats, 0)
}

fn sidecar_pid_label(status: &crate::sidecar::port_file::SidecarStatus) -> String {
    status
        .pid
        .map(|pid| pid.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn sidecar_port_label(status: &crate::sidecar::port_file::SidecarStatus) -> String {
    status
        .port
        .map(|port| port.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

pub(crate) fn format_sidecar_status(status: &crate::sidecar::port_file::SidecarStatus) -> String {
    if status.liveness == crate::sidecar::port_file::SidecarLiveness::NoSidecar {
        // Identity-rejected candidates must NOT vanish into a bare "none":
        // rejected descriptors mean a sidecar EXISTS but refused this
        // caller's root — exactly the evidence a wrong-project investigation
        // needs.
        return match status.detail.as_deref() {
            Some(detail) => format!("Sidecar: none usable ({detail})"),
            None => "Sidecar: none (.symforge/sidecar.* absent)".to_string(),
        };
    }

    let mut line = format!(
        "Sidecar: pid={} port={} state={}",
        sidecar_pid_label(status),
        sidecar_port_label(status),
        status.liveness.as_str(),
    );
    if let Some(detail) = status.detail.as_deref() {
        line.push_str(&format!(" ({detail})"));
    }
    line
}

pub(crate) fn format_sidecar_status_compact(
    status: &crate::sidecar::port_file::SidecarStatus,
) -> String {
    if status.liveness == crate::sidecar::port_file::SidecarLiveness::NoSidecar {
        return match status.detail.as_deref() {
            Some(detail) => format!("Sidecar: none usable ({detail})"),
            None => "Sidecar: none".to_string(),
        };
    }

    format!(
        "Sidecar: {} pid={} port={}",
        status.liveness.as_str(),
        sidecar_pid_label(status),
        sidecar_port_label(status),
    )
}

/// List files changed since the given Unix timestamp.
///
/// If since_ts < loaded_at: return list of all files (entire index is "newer")
/// If since_ts >= loaded_at: return "No changes detected since last index load."
pub fn what_changed_result(index: &LiveIndex, since_ts: i64) -> String {
    let view = index.capture_what_changed_timestamp_view();
    what_changed_timestamp_view(&view, since_ts)
}

/// Render `detect_impact`'s JSON payload wrapped in a short plain-text
/// summary. MCP tool responses here are always text, never raw JSON (house
/// convention — see `tools.rs` module doc); the exact contract shape
/// (contracts/detect-impact.md § Output) is embedded verbatim after the
/// `--- impact payload ---` marker so callers can parse it directly, the same
/// pattern `format_session_cache_hit_body` uses for its cache payload.
pub fn detect_impact_result(
    payload: &serde_json::Value,
    requested_depth: u8,
    effective_depth: u8,
    base_ref: Option<&str>,
    staleness_note: Option<&str>,
) -> String {
    // Counts come from the per-list `pagination` totals, NOT the (capped) arrays,
    // so the summary reports the FULL change/blast size even when the lists are
    // truncated (Wave 1 Fix 1).
    let pagination = &payload["pagination"];
    let changed_files = pagination["changed_files"]["total"].as_u64().unwrap_or(0);
    let changed_symbols = pagination["changed_symbols"]["total"].as_u64().unwrap_or(0);
    let total_blast = pagination["blast_radius"]["total"].as_u64().unwrap_or(0);
    let risk = &payload["risk_summary"];
    let mut summary = format!(
        "Impact analysis: {changed_files} changed file(s), {changed_symbols} changed symbol(s), \
         {total_blast} blast-radius node(s) ({} critical / {} high / {} medium / {} low)",
        risk["critical"], risk["high"], risk["medium"], risk["low"],
    );
    // Self-describing base ref + staleness disclosure (Wave 1 Fix 6).
    if let Some(base) = base_ref {
        summary.push_str(&format!("\nbase: {base}"));
    }
    if let Some(note) = staleness_note {
        summary.push_str(&format!("\nnote: {note}"));
    }
    // Truncation disclosure in the human summary (machine-readable totals live in
    // `pagination`), using the house truncation marker (Wave 1 Fix 1).
    let any_truncated = ["changed_files", "changed_symbols", "blast_radius"]
        .iter()
        .any(|list| pagination[*list]["truncated"].as_bool().unwrap_or(false));
    if any_truncated {
        summary.push_str(&format!(
            "\n{CANONICAL_TRUNCATION_MARKER} one or more lists capped; see `pagination` for full totals and returned counts."
        ));
    }
    let json = serde_json::to_string_pretty(payload).expect("detect_impact payload serializes");
    let mut out = format!("{summary}\n\n--- impact payload ---\n{json}");
    if requested_depth > effective_depth {
        out.push_str(&format!(
            "\n\nWarning: depth clamped to {effective_depth} (requested {requested_depth})."
        ));
    }
    out
}

pub(crate) use crate::index_lifecycle::guidance::changes::{
    what_changed_paths_result, what_changed_timestamp_view,
};
/// Fix 3 (Wave 1): cap the changed/uncommitted-path listing. On a large repo the
/// working-tree listing reached ~100 KB of raw paths; bound it and disclose the
/// omitted count with the house truncation marker.
pub use crate::index_lifecycle::guidance::search_render::search_files_resolve_result_view;

pub fn search_files(index: &LiveIndex, query: &str, limit: usize) -> String {
    let view = index.capture_search_files_view(query, limit, None, None);
    search_files_result_view(&view)
}

pub use crate::index_lifecycle::guidance::search_render::search_files_result_view;

pub(crate) use crate::index_lifecycle::guidance::file_read::render_file_content_bytes;
pub use crate::index_lifecycle::guidance::file_read::{
    GET_FILE_CONTENT_MAX_BYTES, append_nul_byte_warning, cap_file_content_output, file_content,
    file_content_from_indexed_file, file_content_from_indexed_file_with_context, file_content_view,
    not_found_file, not_found_file_match, not_found_symbol,
};

pub fn validate_file_syntax_result(path: &str, file: &IndexedFile) -> String {
    let mut lines = vec![
        format!("Syntax validation: {path}"),
        format!("Language: {}", file.language),
    ];

    match &file.parse_status {
        crate::live_index::ParseStatus::Parsed => {
            lines.push("Status: ok".to_string());
        }
        crate::live_index::ParseStatus::PartialParse { warning } => {
            // SF-003: a TypeScript import-type immediately followed by `[]`
            // (e.g. `import('rxjs').Subscription[]`) is mis-parsed by
            // tree-sitter-typescript 0.23.2 even though it is valid TS. When the
            // partial parse is provably caused only by this grammar limitation,
            // report it as ok with an explanatory note rather than a syntax
            // error.
            if crate::parsing::is_expected_typescript_import_type_array_limitation(
                &file.language,
                &file.content,
                crate::domain::LanguageId::is_tsx_path(&file.relative_path),
            ) {
                lines.push("Status: ok".to_string());
                lines.push(
                    "Note: parser limitation (tree-sitter-typescript 0.23.2 mis-parses an \
                     import-type followed by `[]`; the source is valid TypeScript)"
                        .to_string(),
                );
            } else {
                lines.push("Status: partial".to_string());
                if let Some(diagnostic) = &file.parse_diagnostic {
                    lines.push(format!("Diagnostic: {}", diagnostic.summary()));
                    if let Some((start, end)) = diagnostic.byte_span {
                        lines.push(format!("Byte span: {start}..{end}"));
                    }
                } else {
                    lines.push(format!("Diagnostic: {warning}"));
                }
            }
        }
        crate::live_index::ParseStatus::Failed { error } => {
            lines.push("Status: failed".to_string());
            if let Some(diagnostic) = &file.parse_diagnostic {
                lines.push(format!("Diagnostic: {}", diagnostic.summary()));
                if let Some((start, end)) = diagnostic.byte_span {
                    lines.push(format!("Byte span: {start}..{end}"));
                }
            } else {
                lines.push(format!("Diagnostic: {error}"));
            }
        }
    }

    lines.push(format!("Symbols extracted: {}", file.symbols.len()));
    lines.join("\n")
}

/// Explicit "outside the repository root" error for a path that escapes the
/// indexed repo (e.g. `../../../etc/passwd`). Distinct from `not_found_file` so a
/// traversal attempt is reported as a containment violation rather than a
/// misleading generic miss.
pub fn path_outside_repo(path: &str) -> String {
    format!(
        "Path is outside the repository root: {path}. \
         get_file_content only reads files within the indexed repository; \
         supply a repo-relative path."
    )
}

/// Refusal for a file a secret-detector CONTENT rule excluded from disclosure.
///
/// Names the rule ids, the finding count and, when the gate could compute them,
/// the 1-based finding lines — never a byte of the file — so a false positive
/// can be located and reported (owner ruling 2026-09-29; an earlier revision
/// named nothing, which left every false positive undiagnosable). The refusal
/// text itself offers no self-service bypass; actionable remediation is a
/// separate write tool (`secret_remediate`) reached via `_meta["symforge/withheld"]`
/// (feature 034). A reindex still gives the same verdict on the same bytes.
pub use crate::index_lifecycle::guidance::read_admission_format::content_withheld_by_admission;

/// Refusal for a file a PATH rule excluded: a credential container by name
/// (`.env`, private keys, cloud credential stores). Names the rule.
pub use crate::index_lifecycle::guidance::read_admission_format::content_withheld_by_path_rule;

/// Refusal for a file the admission pipeline could not INSPECT at all — over the
/// deterministic scan budget, or not decodable as searchable text.
///
/// Shares [`content_withheld_by_admission`]'s opening clause, so one anchored
/// predicate classifies both and every contract keyed on that prefix keeps
/// holding. Differs in what follows: these files were never inspected, so
/// there is no rule to name, and they refuse the same way on every read —
/// reindexing will not change them. Names no size, no threshold, no encoding, no rule id, no
/// finding count, and no byte; the wording is identical for both causes, so it
/// does not even distinguish which one applied.
pub use crate::index_lifecycle::guidance::read_admission_format::content_withheld_unscanned;

/// Refusal for a restored file the snapshot verify could not reconcile. Its
/// restored content is withheld rather than served as current, and the file
/// is not reported absent, because it is not.
pub use crate::index_lifecycle::guidance::read_admission_format::unverified_since_restore;

/// Paths the snapshot verify withheld inside `scope`, bounded for display:
/// the count, up to `limit` paths, and how many more there are.
use crate::index_lifecycle::guidance::read_context::withheld_listing;

/// Appended to a project-wide answer when the snapshot verify withheld files
/// inside its scope: those files were not searched, so the answer may be
/// missing hits from them.
pub fn withheld_not_searched_note(
    unverified: &std::collections::BTreeMap<String, String>,
    scope: Option<&str>,
) -> Option<String> {
    crate::index_lifecycle::guidance::read_context::withheld_not_searched_note(unverified, scope)
}
/// A project-wide mutation cannot see references inside withheld files, so
/// it refuses rather than leave them half-renamed.
pub fn project_wide_mutation_refused_unverified(
    tool: &str,
    unverified: &std::collections::BTreeMap<String, String>,
) -> Option<String> {
    let (count, listed) = withheld_listing(unverified, None, 10)?;
    Some(format!(
        "{tool} refused: {count} files are withheld as unverified since restore, and a \
         project-wide change cannot see what they contain: {listed}. A successful re-read \
         releases each one; run index_folder to rebuild the project from source, then retry."
    ))
}

/// Richer "file not found" with suggested similar paths.
/// Call this from tool handlers where the index is available.
pub fn not_found_file_with_suggestions(path: &str, suggestions: &[String]) -> String {
    if suggestions.is_empty() {
        format!("File not found: {path}. Use search_files to find the correct path.")
    } else {
        let top: Vec<&str> = suggestions.iter().take(5).map(|s| s.as_str()).collect();
        format!("File not found: {path}. Did you mean: {}?", top.join(", "))
    }
}

/// Honest "not indexed" response for a path that EXISTS but was deliberately
/// admitted as Tier-2 (metadata-only) or Tier-3 (hard-skipped) rather than
/// parsed. Distinct from `not_found_file`: the file is present on disk, so a
/// bare "File not found" is wrong and confusing. This message names the tier,
/// the skip reason, and the size.
///
/// The recovery sentence is keyed on `raw_read_available` — whether the
/// spec-023 read gate would PERMIT a `get_file_content` raw read of this path.
/// Advising "Use get_file_content" for a path the gate refuses sends the
/// caller into a guaranteed refusal loop (the Tier-2 UX contradiction,
/// testpilot receipt 2026-08-03), so a gated path gets the honest withheld
/// sentence instead.
///
/// Example (Tier 2, raw read available):
///   `Not indexed: package-lock.json is Tier 2 (metadata only) — reason:
///    lockfile, size 406 KB. Use get_file_content for raw reads if you need
///    the contents.`
pub fn not_indexed_skipped_file(
    path: &str,
    tier: AdmissionTier,
    reason: Option<SkipReason>,
    size: Option<u64>,
    raw_read_available: bool,
) -> String {
    let reason_str = reason
        .map(|r| r.to_string())
        .unwrap_or_else(|| "skipped".to_string());
    let size_str = size
        .map(crate::index_lifecycle::guidance::read_context::human_size)
        .unwrap_or_else(|| "unknown".to_string());
    match tier {
        AdmissionTier::Normal => {
            // Not a skip; callers should never reach here with a Normal tier,
            // but degrade gracefully rather than assert.
            not_found_file(path)
        }
        AdmissionTier::MetadataOnly if raw_read_available => format!(
            "Not indexed: {path} is Tier 2 (metadata only) — reason: {reason_str}, size {size_str}. \
             Use get_file_content for raw reads if you need the contents."
        ),
        AdmissionTier::MetadataOnly => format!(
            "Not indexed: {path} is Tier 2 (metadata only) — reason: {reason_str}, size {size_str}. \
             Its contents are withheld by the admission policy — get_file_content will refuse this \
             file, so read it outside SymForge if you need the raw text."
        ),
        AdmissionTier::HardSkip if raw_read_available => format!(
            "Not indexed: {path} is Tier 3 (hard-skipped) — reason: {reason_str}, size {size_str}. \
             This file is not parsed or read; get_file_content may still read it raw if it is a \
             text file within the repository."
        ),
        AdmissionTier::HardSkip => format!(
            "Not indexed: {path} is Tier 3 (hard-skipped) — reason: {reason_str}, size {size_str}. \
             This file is not parsed or read, and its contents are withheld by the admission \
             policy — get_file_content will refuse this file, so read it outside SymForge if you \
             need the raw text."
        ),
    }
}

/// Simple edit-distance score for fuzzy matching (lower is closer).
pub use crate::index_lifecycle::guidance::reference_read::{
    find_dependents_compact_view, find_dependents_dot, find_dependents_mermaid,
    find_dependents_result, find_dependents_result_view, find_references_compact_view,
    find_references_result, find_references_result_view, implementations_result_view,
};

use crate::index_lifecycle::guidance::source::CANONICAL_TRUNCATION_MARKER;
#[cfg(test)]
use crate::index_lifecycle::guidance::source::extract_signature;
#[cfg(test)]
use crate::index_lifecycle::guidance::symbol_context::{
    apply_verbosity, auto_summarize, format_type_dependencies, heuristic_from_name,
};
pub use crate::index_lifecycle::guidance::symbol_context::{
    context_bundle_callees_text, context_bundle_impl_suggestion_tip, context_bundle_result,
    context_bundle_result_view, context_bundle_result_view_with_max_tokens,
    trace_symbol_result_view,
};

pub use crate::index_lifecycle::guidance::source::{
    downgrade_full_completeness_after_truncation, enforce_token_budget,
    enforce_token_budget_flagged,
};

/// The loading-guard refusal: "Index is loading...", then the indexing
/// wording. It says "initial" only for a cold bootstrap, the one load source
/// known to be a project's first index; a warm restore still verifying or a
/// reload in flight is loading too, but is not initial indexing.
pub fn loading_guard_message(cold_bootstrap: bool) -> String {
    let wording = if cold_bootstrap {
        INITIAL_INDEXING_IN_PROGRESS
    } else {
        INDEXING_IN_PROGRESS
    };
    format!("Index is loading... {wording}. Retry the same call shortly.")
}

/// The single wording for a project whose index is loading, said wherever the
/// kind of load is not known: the stdio front's not-ready answer, `status` and
/// `health` in that window, and the SessionStart hook. It claims no waiting:
/// only the stdio front waits, and says so itself.
pub const INDEXING_IN_PROGRESS: &str =
    "indexing of this project is in progress and can take a while on large folders";

/// [`INDEXING_IN_PROGRESS`] for a load known to be a cold bootstrap.
pub const INITIAL_INDEXING_IN_PROGRESS: &str =
    "initial indexing of this project is in progress and can take a while on large folders";

/// Loading-guard prefix while a restored snapshot is being verified. The
/// `Index is loading` start is load-bearing: `is_index_unavailable_output` and
/// `edit_output_is_error` classify the guard by it.
pub const SNAPSHOT_VERIFY_IN_PROGRESS: &str =
    "Index is loading: verifying a restored snapshot against disk";

/// Loading-guard text that names what holds the index. A restored snapshot
/// stays unqueryable until its verification completes (Feature 020 V11: only a
/// complete verified generation is queryable), so say which phase it is in and
/// how far it got instead of a bare "try again shortly".
pub fn loading_guard_message_for(state: &SnapshotVerifyState, cold_bootstrap: bool) -> String {
    let detail = match state {
        SnapshotVerifyState::Pending => "verify=pending".to_string(),
        SnapshotVerifyState::Running(progress) => {
            format!("verify=running {}", progress.describe())
        }
        SnapshotVerifyState::NotNeeded
        | SnapshotVerifyState::Completed(_)
        | SnapshotVerifyState::Failed(_) => {
            return loading_guard_message(cold_bootstrap);
        }
    };
    format!(
        "{SNAPSHOT_VERIFY_IN_PROGRESS} ({detail}). It re-reads the files that changed \
         since the snapshot and publishes them once; restored files stay hidden until it \
         completes. `status` and `health` report its progress."
    )
}

/// Surface-aware empty-index recovery hint (TR-02 / N-5 / FR-011, FR-012).
///
/// Every empty-index / "not loaded" error an agent can reach must name ONLY a
/// recovery action that is callable on the agent's *active* surface. The compact
/// surface forbids `index_folder` (it is not one of the compact-3 tools); naming
/// it there sends the agent into an unrecoverable loop — told to call a tool its
/// surface rejects at dispatch. This single function is the one source of truth
/// for that message, computed from the **active** [`SurfaceProfile`], never a
/// fixed string.
///
/// - [`SurfaceProfile::Compact`]: names only operator-actionable recovery that
///   actually works from a cold / home-cwd start — set `SYMFORGE_WORKSPACE_ROOT`
///   or run `symforge init`, then reconnect — plus the documented
///   `SYMFORGE_SURFACE=full` opt-out. MUST NOT mention `index_folder` (gated),
///   and MUST NOT tell an LLM to "re-launch" (it cannot relaunch its own server).
///   The `Index not loaded.` prefix is load-bearing — empty-index classifiers
///   in tools/edit/golden-replay detect the guard by it.
/// - [`SurfaceProfile::Full`] / [`SurfaceProfile::Meta`]: `index_folder` is
///   callable, so the hint may name it directly.
pub fn empty_index_recovery_hint(profile: SurfaceProfile) -> String {
    match profile {
        SurfaceProfile::Compact => "Index not loaded. SymForge has no resolvable project \
             root. To recover, set SYMFORGE_WORKSPACE_ROOT to the project path in this client's \
             MCP config, or run `symforge init` for this harness, then reconnect. \
             (SYMFORGE_SURFACE=full exposes the full tool surface for manual indexing.)"
            .to_string(),
        SurfaceProfile::Full | SurfaceProfile::Meta => {
            "Index not loaded. Call index_folder to index a directory.".to_string()
        }
    }
}

/// Empty-index guard message computed from the surface served on THIS
/// connection.
///
/// Thin wrapper over [`empty_index_recovery_hint`] that resolves the active
/// surface via [`connection_surface_or_env`]. Existing guard sites and the
/// `loading_guard!` macro call this so the emitted message is always surface-
/// aware without each site having to thread the profile by hand (N-5: one
/// function, no residual hardcoded "Call index_folder" on a compact path).
///
/// D23: `connection_surface_or_env` reads the proxied connection surface when a
/// daemon-side proxied tool call is in flight (bound from the adapter's
/// `CONNECTION_SURFACE_HEADER`), else this process's env. This closes the
/// adapter/daemon env-skew hole where a compact adapter proxied through a full-
/// env daemon was handed a hint naming the gated `index_folder`.
pub fn empty_guard_message() -> String {
    empty_index_recovery_hint(connection_surface_or_env())
}

/// Format a "Token Savings (this session)" section from a `StatsSnapshot`.
///
/// Input: `snap` — the `StatsSnapshot` from `TokenStats::summary()`.
/// Output: a multi-line string listing per-hook-type fire counts and token savings.
///
/// If all counters are zero, returns an empty string (no savings section shown).
/// This is a fail-open function — callers can append the result without checking emptiness.
///
/// ```text
/// ── Token Savings (this session) ──
/// Read:  N fires, ~M tokens saved
/// Edit:  N fires, ~M tokens saved
/// Write: N fires
/// Grep:  N fires, ~M tokens saved
/// Total: ~T tokens saved
/// ```
pub fn format_token_savings(snap: &StatsSnapshot) -> String {
    let total_saved = snap.read_saved_tokens + snap.edit_saved_tokens + snap.grep_saved_tokens;

    // Show section only when at least one hook has fired.
    let any_fires =
        snap.read_fires > 0 || snap.edit_fires > 0 || snap.write_fires > 0 || snap.grep_fires > 0;

    if !any_fires {
        return String::new();
    }

    let mut lines = vec!["── Token Savings (this session) ──".to_string()];

    if snap.read_fires > 0 {
        lines.push(format!(
            "Read:  {} fires, ~{} tokens saved",
            snap.read_fires, snap.read_saved_tokens
        ));
    }
    if snap.edit_fires > 0 {
        lines.push(format!(
            "Edit:  {} fires, ~{} tokens saved",
            snap.edit_fires, snap.edit_saved_tokens
        ));
    }
    if snap.write_fires > 0 {
        lines.push(format!("Write: {} fires", snap.write_fires));
    }
    if snap.grep_fires > 0 {
        lines.push(format!(
            "Grep:  {} fires, ~{} tokens saved",
            snap.grep_fires, snap.grep_saved_tokens
        ));
    }

    lines.push(format!(
        "Total: ~{} tokens saved (vs competent-manual windowed read)",
        total_saved
    ));

    lines.join("\n")
}

/// Format a per-tool token breakdown section showing tokens served, saved, and efficiency ratio.
///
/// Input: `details` — sorted Vec of `(tool_name, tokens_served, tokens_saved)`.
/// Returns empty string when details is empty.
pub fn format_tool_token_breakdown(details: &[(String, u64, u64)]) -> String {
    if details.is_empty() {
        return String::new();
    }

    let total_served: u64 = details.iter().map(|(_, s, _)| s).sum();
    let total_saved: u64 = details.iter().map(|(_, _, s)| s).sum();
    let total_naive = total_served + total_saved;
    let efficiency = if total_served > 0 {
        total_naive as f64 / total_served as f64
    } else {
        1.0
    };
    let reduction_pct = if total_naive > 0 {
        (total_saved as f64 / total_naive as f64 * 100.0) as u64
    } else {
        0
    };

    let mut lines = vec![format!(
        "\u{2500}\u{2500} Session Efficiency (competent-manual baseline) \u{2500}\u{2500}\nTokens served: {}\nCompetent-manual equivalent: {}\nEfficiency: {:.1}x ({reduction_pct}% reduction vs windowed-read baseline)",
        total_served, total_naive, efficiency
    )];

    lines.push(String::new());
    lines.push("\u{2500}\u{2500} Per-Tool Breakdown \u{2500}\u{2500}".to_string());
    let max_name = details.iter().map(|(n, _, _)| n.len()).max().unwrap_or(0);
    for (name, served, saved) in details.iter().take(10) {
        let tool_naive = served + saved;
        let tool_eff = if *served > 0 {
            format!("{:.1}x", tool_naive as f64 / *served as f64)
        } else {
            "-".to_string()
        };
        lines.push(format!(
            "  {:<width$}  {} served, {} saved ({})",
            name,
            served,
            saved,
            tool_eff,
            width = max_name
        ));
    }

    lines.join("\n")
}

/// Format a "Tool Call Counts (this session)" section from per-tool invocation counts.
///
/// Input: `counts` — sorted slice of `(tool_name, count)` from `TokenStats::tool_call_counts()`.
/// Output: a multi-line string. Returns empty string when `counts` is empty.
///
/// ```text
/// ── Tool Call Counts (this session) ──
/// search_text:        12
/// get_file_context:    7
/// get_symbol:          3
/// ```
pub fn format_tool_call_counts(counts: &[(String, usize)]) -> String {
    if counts.is_empty() {
        return String::new();
    }

    let mut lines = vec!["── Tool Call Counts (this session) ──".to_string()];
    // Align counts by padding tool names to the width of the longest name.
    let max_name_len = counts.iter().map(|(n, _)| n.len()).max().unwrap_or(0);
    for (name, count) in counts {
        lines.push(format!("{:<width$}  {}", name, count, width = max_name_len));
    }

    lines.join("\n")
}

/// Competent-manual read baseline: grep-then-~50-line window, calibrated from
/// sf-bench phase-1 token-savings benchmark (2026-06-12, S-vs-M headline tasks).
pub use crate::index_lifecycle::guidance::file_read::COMPETENT_READ_WINDOW_LINES;

/// Whole-file char count below which a competent agent reads the entire file.
pub use crate::index_lifecycle::guidance::file_read::SMALL_FILE_CHAR_THRESHOLD;

/// Line count at or below which responses should stay minimal (outline-only, no hints).
pub use crate::index_lifecycle::guidance::read_context::SMALL_FILE_LINE_THRESHOLD;

/// Whole-file read baseline (legacy naive comparison — kept for transparency).
pub fn whole_file_baseline_chars(raw_chars: usize) -> usize {
    raw_chars
}

/// Windowed read a disciplined agent would do instead of reading the whole file.
pub use crate::index_lifecycle::guidance::file_read::competent_manual_baseline_chars;

pub fn indexed_file_line_count(content: &[u8]) -> usize {
    crate::index_lifecycle::guidance::read_context::indexed_file_line_count(content)
}

pub fn is_small_indexed_file(content_len: usize, line_count: usize) -> bool {
    crate::index_lifecycle::guidance::read_context::is_small_indexed_file(content_len, line_count)
}
/// Default read budget when callers omit `max_tokens` (~50-line competent window).
pub use crate::index_lifecycle::guidance::file_read::default_read_max_tokens;

/// Resolve an explicit or default token budget for read-like tools.
pub use crate::index_lifecycle::guidance::file_read::resolve_read_max_tokens;

/// Estimated tokens from a character count (`~chars/4` approximation).
///
/// Coarse heuristic, NOT a measured token count. Callers that surface the
/// result to an agent label it as an estimate (the savings footer uses `~N
/// tokens` framing) so no figure is presented as exact/measured (010 N-4).
pub use crate::index_lifecycle::guidance::file_read::estimate_tokens_from_chars;

pub fn saved_tokens_whole_file(response_chars: usize, raw_chars: usize) -> u64 {
    crate::index_lifecycle::guidance::read_context::saved_tokens_whole_file(
        response_chars,
        raw_chars,
    )
}

pub fn saved_tokens_vs_competent_manual(response_chars: usize, raw_chars: usize) -> u64 {
    crate::index_lifecycle::guidance::read_context::saved_tokens_vs_competent_manual(
        response_chars,
        raw_chars,
    )
}
/// Baseline for search/reference listing tools without a single raw file length.
pub fn estimate_listing_baseline_chars(output_chars: usize) -> usize {
    let hits = (output_chars / 80).max(1);
    let grep = hits.saturating_mul(120);
    let windows = (hits / 5 + 1).saturating_mul(competent_manual_baseline_chars(4000));
    grep.saturating_add(windows).max(output_chars)
}

/// Estimate tokens saved by a structured response vs raw file content.
/// Returns a one-line footer string, or empty string if no meaningful savings.
pub fn compact_savings_footer(response_chars: usize, raw_chars: usize) -> String {
    crate::index_lifecycle::guidance::read_context::compact_savings_footer(
        response_chars,
        raw_chars,
    )
}

pub use crate::index_lifecycle::guidance::source::format_session_cache_hit_body;

/// Dedup hint when agent forces a re-fetch of content already in session (011 US4).
pub use crate::index_lifecycle::guidance::file_read::append_dedup_hint_footer;

/// Format a "Hook Adoption (current session)" section from hook-time workflow counters.
pub(crate) fn format_hook_adoption(snap: &HookAdoptionSnapshot) -> String {
    if snap.is_empty() {
        return String::new();
    }

    let total = snap.total_attempts();
    let routed = snap.total_routed();
    let percent = if total == 0 {
        0
    } else {
        ((routed as f64 / total as f64) * 100.0).round() as usize
    };

    let mut lines = vec![
        "── Hook Adoption (current session) ──".to_string(),
        format!("Owned workflows routed: {routed}/{total} ({percent}%)"),
    ];

    let total_no_sidecar = snap.source_read.no_sidecar
        + snap.source_search.no_sidecar
        + snap.repo_start.no_sidecar
        + snap.prompt_context.no_sidecar
        + snap.post_edit_impact.no_sidecar;
    let total_sidecar_error = snap.source_read.sidecar_error
        + snap.source_search.sidecar_error
        + snap.repo_start.sidecar_error
        + snap.prompt_context.sidecar_error
        + snap.post_edit_impact.sidecar_error;
    let fail_open_total = snap.total_fail_open();
    if fail_open_total > 0 {
        lines.push(format!(
            "Fail-open outcomes: {fail_open_total} (no sidecar {total_no_sidecar}, sidecar errors {total_sidecar_error})"
        ));
    } else {
        lines.push("Fail-open outcomes: 0".to_string());
    }

    // Show daemon fallback total if any occurred.
    let total_daemon = snap.source_read.daemon_fallback
        + snap.source_search.daemon_fallback
        + snap.repo_start.daemon_fallback
        + snap.prompt_context.daemon_fallback
        + snap.post_edit_impact.daemon_fallback;
    if total_daemon > 0 {
        lines.push(format!("Daemon fallback routed: {total_daemon}"));
        lines.push(
            "Daemon fallback counts as routed work: the hook reached the daemon even though the sidecar was unavailable."
                .to_string(),
        );
    }

    let mut push_workflow_line =
        |label: &str, counts: &crate::cli::hook::WorkflowAdoptionCounts| {
            if counts.total() == 0 {
                return;
            }
            let mut parts = vec![format!("routed {}", counts.routed)];
            if counts.daemon_fallback > 0 {
                parts.push(format!("daemon fallback {}", counts.daemon_fallback));
            }
            if counts.fail_open() > 0 && counts.no_sidecar > 0 {
                parts.push(format!("no sidecar {}", counts.no_sidecar));
            }
            if counts.fail_open() > 0 && counts.sidecar_error > 0 {
                parts.push(format!("sidecar errors {}", counts.sidecar_error));
            }
            lines.push(format!("{label}: {}", parts.join(", ")));
        };

    push_workflow_line("Source read", &snap.source_read);
    push_workflow_line("Source search", &snap.source_search);
    push_workflow_line("Repo start", &snap.repo_start);
    push_workflow_line("Prompt context", &snap.prompt_context);
    push_workflow_line("Post-edit impact", &snap.post_edit_impact);

    if let Some(first) = snap.first_repo_start {
        lines.push(format!("First repo start: {}", first.label()));
    }

    // Show a hint when all fail-open outcomes are due to no-sidecar.
    if snap.total_fail_open() > 0
        && snap.total_routed() == 0
        && total_daemon == 0
        && total_sidecar_error == 0
    {
        lines.push(String::new());
        lines.push("⚠ All hook attempts failed open (no sidecar found).".to_string());
        lines.push("  Start SymForge as an MCP server or run 'symforge daemon start'.".to_string());
    } else if fail_open_total > 0 && total_sidecar_error == 0 {
        lines.push(String::new());
        lines.push(
            "Fail-open here is mostly benign: hooks fired before a sidecar was reachable or on workflows intentionally left pass-through."
                .to_string(),
        );
    } else if total_sidecar_error > 0 {
        lines.push(String::new());
        lines.push(
            "Actionable note: sidecar errors are real routing failures and worth investigating separately from no-sidecar outcomes."
                .to_string(),
        );
    }

    lines.join("\n")
}

pub use crate::index_lifecycle::guidance::search_render::{
    ExploreResultViewInput, explore_result_view,
};

/// Format git temporal data for a single file: churn, ownership, co-changes, last commit.
pub fn co_changes_result_view(
    path: &str,
    history: &crate::live_index::git_temporal::GitFileHistory,
    limit: usize,
) -> String {
    let mut lines = Vec::new();

    lines.push(format!("Git temporal data for {path}"));
    lines.push(String::new());

    // Churn
    lines.push(format!(
        "Churn score: {:.2} ({} commits)",
        history.churn_score, history.commit_count
    ));

    // Last commit
    let c = &history.last_commit;
    lines.push(format!(
        "Last commit: {} {} — {} ({})",
        c.hash, c.timestamp, c.message_head, c.author
    ));
    lines.push(String::new());

    // Ownership
    if !history.contributors.is_empty() {
        lines.push("Ownership:".to_string());
        for contrib in &history.contributors {
            lines.push(format!(
                "  {}: {} commits ({:.0}%)",
                contrib.author, contrib.commit_count, contrib.percentage
            ));
        }
        lines.push(String::new());
    }

    // Co-changes
    if history.co_changes.is_empty() {
        lines.push(
            "No high-confidence co-changing files detected (needs at least 2 shared commits and Jaccard >= 0.15)."
                .to_string(),
        );
        if !history.weak_co_changes.is_empty() {
            lines.push(String::new());
            lines.push(format!(
                "Low-confidence candidates (top {}):",
                limit.min(history.weak_co_changes.len())
            ));
            for entry in history.weak_co_changes.iter().take(limit) {
                lines.push(format!(
                    "  {:<50} coupling: {:.3}  ({} shared commits)",
                    entry.path, entry.coupling_score, entry.shared_commits
                ));
            }
            lines.push(
                "These missed the strong co-change threshold and are advisory only.".to_string(),
            );
        }
    } else {
        lines.push(format!(
            "Co-changing files (top {}):",
            limit.min(history.co_changes.len())
        ));
        for entry in history.co_changes.iter().take(limit) {
            lines.push(format!(
                "  {:<50} coupling: {:.3}  ({} shared commits)",
                entry.path, entry.coupling_score, entry.shared_commits
            ));
        }
    }

    lines.join("\n")
}

/// Compute the impact summary for a just-edited path: the distinct dependent
/// **file** count and the top-K co-change partner paths.
///
/// - Dependents use `capture_find_dependents_view(path).files.len()` — the count
///   of distinct importing/referencing files (matching the `find_dependents`
///   tool), NOT the raw per-reference count which double-counts a file that holds
///   multiple references.
/// - Co-changes are present only when `temporal.state` is `Ready` and the edited
///   path has a non-empty strong `co_changes` list; otherwise the returned vector
///   is empty (degrading the footer to `[impact: N dependents]`). The path is
///   forward-slash normalized before the temporal lookup to match the temporal
///   index key space.
///
/// `temporal` is passed in (rather than read off `index`) because the git
/// temporal snapshot lives on the shared index handle, not the `LiveIndex` read
/// snapshot. The caller (`append_impact_footer`) holds the handle and supplies
/// both sources.
pub fn edit_impact_summary(
    index: &LiveIndex,
    temporal: &crate::live_index::git_temporal::GitTemporalIndex,
    path: &str,
) -> (usize, Vec<String>) {
    const COCHANGE_LIMIT: usize = 3;

    let deps = index.capture_find_dependents_view(path).files.len();

    let normalized = path.replace('\\', "/");
    let cochanges = if temporal.state == crate::live_index::git_temporal::GitTemporalState::Ready {
        temporal
            .files
            .get(&normalized)
            .filter(|history| !history.co_changes.is_empty())
            .map(|history| {
                history
                    .co_changes
                    .iter()
                    .take(COCHANGE_LIMIT)
                    .map(|entry| entry.path.clone())
                    .collect::<Vec<String>>()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    (deps, cochanges)
}

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

/// Format symbol-level diff between two git refs.
///
/// `live` is required because EVERY content read in this function is a
/// disclosure lane, not just the working-tree one. An earlier version of this
/// comment scoped the hazard to "uncommitted mode reads the WORKING TREE",
/// gated that single read, and left the two `file_at_ref` reads beside it
/// ungated — so a security-demoted file's symbol names and signatures rendered
/// straight into any committed-vs-committed diff. Both git-object reads now go
/// through the same admission gate.
///
/// A refused file is reported as withheld rather than rendered as empty —
/// rendering it empty would state, falsely, that every one of its symbols was
/// removed.
pub fn diff_symbols_result_view(
    base: &str,
    target: &str,
    changed_files: &[&str],
    repo: &crate::git::GitRepo,
    live: &crate::live_index::LiveIndex,
    compact: bool,
    summary_only: bool,
) -> String {
    let view = crate::index_lifecycle::guidance::changes::capture_symbol_diff(
        base,
        target,
        changed_files,
        |reference, path| {
            if reference.is_empty() {
                crate::protocol::read_gate::admit_worktree_text(live, repo, path)
            } else {
                crate::protocol::read_gate::admit_git_text(live, repo, reference, path)
            }
        },
    );
    crate::index_lifecycle::guidance::changes::render_symbol_diff(&view, compact, summary_only)
}

/// Try to extract a declaration name from a line of code.
#[cfg(test)]
pub(crate) use crate::index_lifecycle::guidance::symbol_context::extract_declaration_name;

#[cfg(test)]
mod sfb15_tests {
    use super::*;

    #[test]
    fn impact_footer_elides_partner_paths_carrying_classifier_sentinels() {
        // A co-change partner whose path embeds a `classify_edit_output`
        // `.contains` sentinel (here `unavailable`) must NOT leak into the
        // footer, or the three `_tool` wrappers would re-classify a successful
        // destructive edit as a failure.
        let footer = impact_footer(
            2,
            &[
                "src/unavailable.rs".to_string(),
                "src/protocol/format.rs".to_string(),
            ],
        );
        assert!(
            !footer.contains("unavailable"),
            "footer must not carry a sentinel-bearing partner path: {footer}"
        );
        assert!(
            footer.contains("src/protocol/format.rs"),
            "safe partner should remain: {footer}"
        );
        assert!(footer.starts_with("[impact: 2 dependents"));

        // When every partner is sentinel-bearing, the cochanges clause is
        // dropped entirely, degrading to the dependents-only form.
        let all_filtered = impact_footer(3, &["build/unavailable_state.rs".to_string()]);
        assert_eq!(all_filtered, "[impact: 3 dependents]");
    }

    #[test]
    fn explore_result_view_filters_weak_trivial_symbols_and_doc_only_patterns() {
        let symbol_hits = vec![
            (
                "new".to_string(),
                "fn".to_string(),
                "src/cache/builder.rs".to_string(),
            ),
            (
                "TokenCache".to_string(),
                "struct".to_string(),
                "src/cache/token_cache.rs".to_string(),
            ),
        ];
        let text_hits = vec![
            (
                "src/cache/token_cache.rs".to_string(),
                "/// Token cache mentions authentication.".to_string(),
                1,
            ),
            (
                "src/cache/token_cache.rs".to_string(),
                "let cache = TokenCache::new();".to_string(),
                8,
            ),
        ];
        let related_files = vec![("src/cache/token_cache.rs".to_string(), 2)];
        let symbol_scores = vec![0.20, 0.95];

        let output = explore_result_view(ExploreResultViewInput {
            label: "cache",
            symbol_hits: &symbol_hits,
            text_hits: &text_hits,
            related_files: &related_files,
            enriched_symbols: &[],
            symbol_impls: &[],
            symbol_deps: &[],
            derived_seed_terms: &[],
            derived_symbols: &[],
            derived_seed_files: &[],
            enriched_imports: &[],
            symbol_scores: &symbol_scores,
            depth: 1,
        });

        assert!(
            !output.contains("fn new"),
            "weak trivial symbol should be filtered: {output}"
        );
        assert!(
            output.contains("struct TokenCache"),
            "high-signal project symbol should remain visible: {output}"
        );
        assert!(
            output.contains("reason: strong match"),
            "visible suggestions should carry concise reason text: {output}"
        );
        assert!(
            !output.contains("/// Token cache"),
            "doc-only pattern hit should be filtered from explore guidance: {output}"
        );
        assert!(
            output.contains("let cache = TokenCache::new();"),
            "real code pattern should remain visible: {output}"
        );
    }

    #[test]
    fn explore_result_view_keeps_trivial_symbol_when_strongly_contextualized() {
        let symbol_hits = vec![(
            "new".to_string(),
            "fn".to_string(),
            "src/cache/builder.rs".to_string(),
        )];
        let symbol_scores = vec![0.92];

        let output = explore_result_view(ExploreResultViewInput {
            label: "builder",
            symbol_hits: &symbol_hits,
            text_hits: &[],
            related_files: &[],
            enriched_symbols: &[],
            symbol_impls: &[],
            symbol_deps: &[],
            derived_seed_terms: &[],
            derived_symbols: &[],
            derived_seed_files: &[],
            enriched_imports: &[],
            symbol_scores: &symbol_scores,
            depth: 1,
        });

        assert!(
            output.contains("fn new"),
            "strongly contextualized trivial symbol should remain visible: {output}"
        );
        assert!(
            output.contains("reason: strong match"),
            "kept trivial symbol should explain why it was suggested: {output}"
        );
    }
}

#[cfg(test)]
mod tests;
