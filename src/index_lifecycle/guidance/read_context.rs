//! Transport-independent repository map and file-context rendering.

use crate::domain::ReferenceKind;
use crate::domain::index::{AdmissionTier, SkippedFile};
use crate::live_index::{RepoOutlineFileView, RepoOutlineView};

pub fn file_tree_view(files: &[RepoOutlineFileView], path: &str, depth: u32) -> String {
    let depth = depth.min(5);
    let prefix = path.trim_matches('/');

    // Collect all files whose relative_path starts with the path prefix.
    let matching_files: Vec<&RepoOutlineFileView> = files
        .iter()
        .filter(|file| {
            let p = file.relative_path.as_str();
            if prefix.is_empty() {
                true
            } else {
                p.starts_with(prefix)
                    && (p.len() == prefix.len() || p.as_bytes().get(prefix.len()) == Some(&b'/'))
            }
        })
        .collect();

    if matching_files.is_empty() {
        return format!(
            "No source files found under '{}'",
            if prefix.is_empty() { "." } else { prefix }
        );
    }

    // Build a tree: BTreeMap from directory path -> Vec<(filename, lang, symbol_count)>
    // Node entries are keyed by their path component at each level.
    use std::collections::BTreeMap;

    // Strip the prefix from all paths before building the tree.
    let strip_len = if prefix.is_empty() {
        0
    } else {
        prefix.len() + 1
    };
    let stripped: Vec<(&str, &RepoOutlineFileView)> = matching_files
        .into_iter()
        .map(|file| {
            let p = file.relative_path.as_str();
            (
                if p.len() >= strip_len {
                    &p[strip_len..]
                } else {
                    p
                },
                file,
            )
        })
        .collect();

    // Recursively build tree lines.
    fn build_lines(
        entries: &[(&str, &RepoOutlineFileView)],
        current_depth: u32,
        max_depth: u32,
        indent: usize,
    ) -> Vec<String> {
        // Group by first path component.
        let mut dirs: BTreeMap<&str, Vec<(&str, &RepoOutlineFileView)>> = BTreeMap::new();
        let mut files_here: Vec<(&str, &RepoOutlineFileView)> = Vec::new();

        for (rel, file) in entries {
            if let Some(slash) = rel.find('/') {
                let dir_part = &rel[..slash];
                let rest = &rel[slash + 1..];
                dirs.entry(dir_part).or_default().push((rest, file));
            } else {
                files_here.push((rel, file));
            }
        }

        let pad = "  ".repeat(indent);
        let mut lines = Vec::new();

        // Files at this level
        files_here.sort_by_key(|(name, _)| *name);
        for (name, file) in &files_here {
            let sym_count = file.symbol_count;
            let sym_label = if sym_count == 1 { "symbol" } else { "symbols" };
            let tag = file.noise_class.tag();
            if tag.is_empty() {
                lines.push(format!(
                    "{}{} [{}]  ({} {})",
                    pad, name, file.language, sym_count, sym_label
                ));
            } else {
                lines.push(format!(
                    "{}{} [{}]  ({} {}) {}",
                    pad, name, file.language, sym_count, sym_label, tag
                ));
            }
        }

        // Directories at this level
        for (dir_name, children) in &dirs {
            let file_count = count_files(children);
            let sym_count: usize = children.iter().map(|(_, f)| f.symbol_count).sum();
            let sym_label = if sym_count == 1 { "symbol" } else { "symbols" };

            if current_depth >= max_depth {
                // Collapsed — just show summary line
                lines.push(format!(
                    "{}{}/  ({} files, {} {})",
                    pad, dir_name, file_count, sym_count, sym_label
                ));
            } else {
                lines.push(format!(
                    "{}{}/  ({} files, {} {})",
                    pad, dir_name, file_count, sym_count, sym_label
                ));
                let sub_lines = build_lines(children, current_depth + 1, max_depth, indent + 1);
                lines.extend(sub_lines);
            }
        }

        lines
    }

    fn count_files(entries: &[(&str, &RepoOutlineFileView)]) -> usize {
        let mut count = 0;
        for (rel, _) in entries {
            if rel.contains('/') {
                // nested
            } else {
                count += 1;
            }
        }
        // also count files in sub-directories
        let mut dirs: std::collections::HashMap<&str, Vec<(&str, &RepoOutlineFileView)>> =
            std::collections::HashMap::new();
        for (rel, file) in entries {
            if let Some(slash) = rel.find('/') {
                dirs.entry(&rel[..slash])
                    .or_default()
                    .push((&rel[slash + 1..], file));
            }
        }
        for children in dirs.values() {
            count += count_files(children);
        }
        count
    }

    fn count_dirs(entries: &[(&str, &RepoOutlineFileView)]) -> usize {
        let mut dirs: std::collections::HashSet<&str> = std::collections::HashSet::new();
        let mut sub_entries: std::collections::HashMap<&str, Vec<(&str, &RepoOutlineFileView)>> =
            std::collections::HashMap::new();
        for (rel, file) in entries {
            if let Some(slash) = rel.find('/') {
                let dir_name = &rel[..slash];
                dirs.insert(dir_name);
                sub_entries
                    .entry(dir_name)
                    .or_default()
                    .push((&rel[slash + 1..], file));
            }
        }
        let mut total = dirs.len();
        for children in sub_entries.values() {
            total += count_dirs(children);
        }
        total
    }

    let body_lines = build_lines(&stripped, 1, depth, 0);

    let total_files = stripped.len();
    let total_dirs = count_dirs(&stripped);
    let total_symbols: usize = stripped.iter().map(|(_, f)| f.symbol_count).sum();
    let sym_label = if total_symbols == 1 {
        "symbol"
    } else {
        "symbols"
    };

    let mut output = body_lines;
    output.push(format!(
        "{} directories, {} files, {} {}",
        total_dirs, total_files, total_symbols, sym_label
    ));

    output.join("\n")
}

/// Format a byte count as a human-readable size string.
pub(crate) fn human_size(bytes: u64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.1} GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.0} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

/// Like `file_tree_view` but also incorporates skipped files:
/// - Tier 2 (MetadataOnly) files appear in the tree with a `[skipped: {reason}, {size}]` tag.
/// - Tier 3 (HardSkip) files do NOT appear in the tree; instead a footer line is appended:
///   `{N} hard-skipped artifacts not shown (>100MB)`
pub fn file_tree_view_with_skipped(
    files: &[RepoOutlineFileView],
    skipped: &[SkippedFile],
    path: &str,
    depth: u32,
) -> String {
    // Separate Tier 2 and Tier 3 skipped files, filtered to the path prefix.
    let prefix = path.trim_matches('/');
    let tier2: Vec<&SkippedFile> = skipped
        .iter()
        .filter(|sf| {
            sf.decision.tier == AdmissionTier::MetadataOnly
                && (prefix.is_empty()
                    || sf.path.starts_with(prefix)
                        && (sf.path.len() == prefix.len()
                            || sf.path.as_bytes().get(prefix.len()) == Some(&b'/')))
        })
        .collect();
    let tier3_count = skipped
        .iter()
        .filter(|sf| {
            sf.decision.tier == AdmissionTier::HardSkip
                && (prefix.is_empty()
                    || sf.path.starts_with(prefix)
                        && (sf.path.len() == prefix.len()
                            || sf.path.as_bytes().get(prefix.len()) == Some(&b'/')))
        })
        .count();

    // Build the base tree from indexed files.
    let base = if tier2.is_empty() && files.is_empty() {
        file_tree_view(files, path, depth)
    } else {
        // Build augmented file list: convert Tier 2 skipped files into synthetic
        // RepoOutlineFileView entries so they appear in the tree with the skip tag appended.
        // We render the base tree first, then inject Tier 2 entries separately.
        file_tree_view(files, path, depth)
    };

    // If there are no indexed files and no Tier 2 skipped files, the base already handles it.
    // We need to inject Tier 2 entries into the output.
    // Strategy: build Tier 2 lines separately and splice into base before the footer.
    // Simpler approach: re-render with Tier 2 files appended as extra lines after the base tree body.

    // Split base output into body lines and footer (last line is always the summary).
    let mut lines: Vec<String> = base.lines().map(String::from).collect();
    let footer = if lines.len() > 1 { lines.pop() } else { None };

    // Build Tier 2 file lines. Each gets placed at the correct indentation by stripping the prefix.
    let strip_len = if prefix.is_empty() {
        0
    } else {
        prefix.len() + 1
    };
    let mut tier2_lines: Vec<(String, String)> = tier2
        .iter()
        .map(|sf| {
            let p = sf.path.as_str();
            let rel = if p.len() >= strip_len {
                &p[strip_len..]
            } else {
                p
            };
            let reason = sf
                .decision
                .reason
                .as_ref()
                .map(|r| r.to_string())
                .unwrap_or_else(|| "skipped".to_string());
            let tag = format!("[skipped: {}, {}]", reason, human_size(sf.size));
            (rel.to_string(), tag)
        })
        .collect();
    tier2_lines.sort_by(|a, b| a.0.cmp(&b.0));

    for (rel, tag) in &tier2_lines {
        // Compute indentation from path depth.
        let depth_level = rel.chars().filter(|&c| c == '/').count();
        let pad = "  ".repeat(depth_level);
        let filename = rel.rsplit('/').next().unwrap_or(rel.as_str());
        lines.push(format!("{}{}  {}", pad, filename, tag));
    }

    // Re-append footer, then add Tier 3 footer if needed.
    if let Some(f) = footer {
        lines.push(f);
    }
    if tier3_count > 0 {
        let artifact_label = if tier3_count == 1 {
            "artifact"
        } else {
            "artifacts"
        };
        lines.push(format!(
            "{} hard-skipped {} not shown (>100MB)",
            tier3_count, artifact_label
        ));
    }

    lines.join("\n")
}

pub fn repo_outline_view(view: &RepoOutlineView, project_name: &str) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "{project_name}  ({} files, {} symbols)",
        view.total_files, view.total_symbols
    ));

    // Always show full relative paths for orientation (not just disambiguated basenames).
    let path_width = view
        .files
        .iter()
        .map(|f| f.relative_path.len())
        .max()
        .unwrap_or(20)
        .clamp(20, 50);

    for file in &view.files {
        lines.push(format!(
            "  {:<width$} {:<12} {} symbols",
            file.relative_path,
            file.language.to_string(),
            file.symbol_count,
            width = path_width
        ));
    }

    lines.join("\n")
}

pub fn build_with_budget(items: &[String], max_bytes: u64) -> (String, usize) {
    if max_bytes == 0 || items.is_empty() {
        return (items.join("\n"), 0);
    }

    let mut included = Vec::new();
    let mut used_bytes: u64 = 0;

    for (i, item) in items.iter().enumerate() {
        // Each item costs: len + 1 newline (except the last).
        let item_cost = item.len() as u64 + if i + 1 < items.len() { 1 } else { 0 };
        if used_bytes + item_cost > max_bytes && !included.is_empty() {
            // Would exceed budget — stop here.
            let remaining = items.len() - included.len();
            let mut text = included.join("\n");
            text.push_str(&budget_truncation_suffix(max_bytes, remaining));
            return (text, remaining);
        }
        used_bytes += item_cost;
        included.push(item.as_str());
    }

    // After the loop: if fewer items were included than available (e.g. because
    // the very first item exceeded max_bytes and forced inclusion while the rest
    // were silently dropped), always append the truncation suffix so callers
    // know output was cut short.
    if included.len() < items.len() {
        let remaining = items.len() - included.len();
        let mut text = included.join("\n");
        text.push_str(&budget_truncation_suffix(max_bytes, remaining));
        return (text, remaining);
    }

    (included.join("\n"), 0)
}

const CANONICAL_TRUNCATION_MARKER: &str = "[truncated]";
const APPROX_BYTES_PER_TOKEN: u64 = 4;

fn approx_tokens_from_bytes(bytes: u64) -> u64 {
    bytes.saturating_add(APPROX_BYTES_PER_TOKEN - 1) / APPROX_BYTES_PER_TOKEN
}

fn budget_truncation_suffix(max_bytes: u64, remaining: usize) -> String {
    let max_tokens = approx_tokens_from_bytes(max_bytes);
    format!(
        "\n{CANONICAL_TRUNCATION_MARKER} Truncated at ~{max_tokens} tokens. {remaining} additional output line(s) not shown."
    )
}

pub(crate) fn is_intra_workspace_path(path: &str) -> bool {
    if path.contains(':') || path.starts_with('/') || path.starts_with('\\') {
        return false;
    }
    !path
        .replace('\\', "/")
        .split('/')
        .any(|segment| segment == "..")
}
fn get_dir_2level(path: &str) -> String {
    let p = std::path::Path::new(path);
    let components: Vec<_> = p.components().collect();

    if components.len() <= 1 {
        // Root-level file.
        return "(root)".to_string();
    }

    // Take at most 2 directory components (exclude the file name).
    let dir_components: Vec<_> = components[..components.len() - 1].iter().take(2).collect();
    dir_components
        .iter()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/")
}

pub(crate) fn repo_map_text_for_generation(
    generation: &crate::live_index::PublishedGeneration,
) -> String {
    let guard = generation.live.as_ref();

    let total_files = guard.file_count();
    let total_symbols = guard.symbol_count();

    // Collect language breakdown.
    let mut lang_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    // Collect per-directory stats (2-level max).
    let mut dir_file_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut dir_symbol_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();

    for (path, file) in guard.all_files() {
        // Skip files with absolute paths (outside project root, e.g., Windows memory files).
        if !is_intra_workspace_path(path) {
            continue;
        }

        // Language breakdown.
        let lang = format!("{:?}", file.language);
        *lang_counts.entry(lang).or_insert(0) += 1;

        // Directory (up to 2 levels).
        let dir = get_dir_2level(path);
        *dir_file_counts.entry(dir.clone()).or_insert(0) += 1;
        *dir_symbol_counts.entry(dir).or_insert(0) += file.symbols.len();
    }

    // Build header.
    let mut lang_parts: Vec<String> = lang_counts
        .iter()
        .map(|(k, v)| format!("{}: {}", k, v))
        .collect();
    lang_parts.sort();

    let mut lines: Vec<String> = Vec::new();
    lines.push(format!(
        "Index: {} files, {} symbols  [{}]",
        total_files,
        total_symbols,
        lang_parts.join(", ")
    ));
    lines.push(String::new());

    // Sort directories and emit tree.
    let mut dirs: Vec<String> = dir_file_counts.keys().cloned().collect();
    dirs.sort();

    for dir in &dirs {
        let file_count = dir_file_counts[dir];
        let sym_count = dir_symbol_counts[dir];
        lines.push(format!(
            "  {:<35}  {:>3} files   {:>5} symbols",
            dir, file_count, sym_count
        ));
    }

    // Key entry points: top-level structs/traits/interfaces/enums in src/ (depth 0, limit 10).
    {
        let mut entry_points: Vec<(String, String, String)> = Vec::new(); // (kind, name, path)
        for (path, file) in guard.all_files() {
            // Exclude paths from other indexed workspaces — same guard as the
            // directory-stats loop above; without it the key-types section
            // leaks symbols from unrelated projects.
            if !is_intra_workspace_path(path) {
                continue;
            }
            // Only source code, skip docs/tests/vendor
            let pl = path.to_ascii_lowercase();
            if pl.ends_with(".md")
                || pl.contains("/docs/")
                || pl.contains("vendor/")
                || pl.contains("node_modules/")
            {
                continue;
            }
            for sym in &file.symbols {
                if sym.depth == 0 {
                    match sym.kind {
                        crate::domain::SymbolKind::Struct
                        | crate::domain::SymbolKind::Trait
                        | crate::domain::SymbolKind::Interface
                        | crate::domain::SymbolKind::Enum
                        | crate::domain::SymbolKind::Class => {
                            entry_points.push((
                                sym.kind.to_string(),
                                sym.name.clone(),
                                path.to_string(),
                            ));
                        }
                        _ => {}
                    }
                }
            }
        }
        if !entry_points.is_empty() {
            // Importance ranking (feature 007, US3): rank entry-point lines by
            // their containing file's importance rather than alphabetically.
            //
            // rank_key(file) = (dependent_count DESC, churn_score DESC,
            //                   relative_path ASC, symbol_name ASC)
            //
            // The `relative_path ASC` key is the contract's deterministic
            // tie-break; `symbol_name ASC` is the additional innermost key that
            // keeps order stable when one file contributes several top-level
            // types (multiple entry-point lines share a path). Identical index
            // state therefore always yields identical order (FR-017).

            // Distinct importing-file count, memoized per distinct entry-point
            // path. The candidate set is bounded (only files with top-level
            // types), so this is O(candidates × refs), never O(all_files²).
            // Keyed by owned path so the memo does not borrow `entry_points`
            // (which must stay mutably sortable/truncatable below).
            let mut dep_counts: std::collections::HashMap<String, usize> =
                std::collections::HashMap::new();
            for (_, _, path) in &entry_points {
                if !dep_counts.contains_key(path) {
                    let distinct: std::collections::HashSet<&str> = guard
                        .find_dependents_for_file(path)
                        .into_iter()
                        .map(|(file_path, _)| file_path)
                        .collect();
                    dep_counts.insert(path.clone(), distinct.len());
                }
            }

            // Churn from the lock-free temporal snapshot; 0.0 when temporal is
            // not Ready or the file is absent (read-only — no frecency bump).
            let temporal = generation.code_signals.temporal.as_ref();
            let churn_of = |path: &str| -> f32 {
                if generation.code_signals.state
                    == crate::live_index::git_temporal::GitTemporalState::Ready
                {
                    temporal
                        .files
                        .get(path)
                        .map(|history| history.churn_score)
                        .unwrap_or(0.0)
                } else {
                    0.0
                }
            };

            entry_points.sort_by(|a, b| {
                let (a_kind, a_name, a_path) = a;
                let (b_kind, b_name, b_path) = b;
                let a_dep = dep_counts.get(a_path.as_str()).copied().unwrap_or(0);
                let b_dep = dep_counts.get(b_path.as_str()).copied().unwrap_or(0);
                // dependent_count DESC
                b_dep
                    .cmp(&a_dep)
                    // churn_score DESC (f32 in [0,1], never NaN; Equal fallback
                    // is harmless because the path/name keys below are total).
                    .then_with(|| {
                        churn_of(b_path)
                            .partial_cmp(&churn_of(a_path))
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    // relative_path ASC (deterministic tie-break)
                    .then_with(|| a_path.cmp(b_path))
                    // symbol_name ASC (stable order for multi-type files)
                    .then_with(|| a_name.cmp(b_name))
                    // kind ASC (final guard; identical (path,name) is unusual
                    // but keeps the order total either way).
                    .then_with(|| a_kind.cmp(b_kind))
            });
            entry_points.truncate(15);
            lines.push(String::new());
            lines.push("Key types:".to_string());
            for (kind, name, path) in &entry_points {
                // Annotate high-fan-in files: `(→N)` iff distinct dependents N>=2.
                let dep_count = dep_counts.get(path.as_str()).copied().unwrap_or(0);
                if dep_count >= 2 {
                    lines.push(format!("  {kind} {name}  ({path}) (→{dep_count})"));
                } else {
                    lines.push(format!("  {kind} {name}  ({path})"));
                }
            }
            if entry_points.len() == 15 {
                lines.push("  ...".to_string());
            }
        }
    }

    // Apply budget (1000 tokens = 4000 bytes).
    // Medium repos (up to ~70 directories) fit without truncation.
    let (text, _) = build_with_budget(&lines, 4000);

    text
}

#[derive(Clone, Copy)]
pub(crate) enum ContextSourceAuthority {
    DiskRefreshed,
    CurrentIndex,
}

fn context_source_authority_label(authority: ContextSourceAuthority) -> &'static str {
    match authority {
        ContextSourceAuthority::DiskRefreshed => "disk-refreshed",
        ContextSourceAuthority::CurrentIndex => "current index",
    }
}

pub(crate) fn parse_state_label(file: &crate::live_index::store::IndexedFile) -> &'static str {
    match &file.parse_status {
        crate::live_index::store::ParseStatus::Parsed => "parsed",
        crate::live_index::store::ParseStatus::PartialParse { .. } => {
            // SF-004: a partial parse caused only by Angular template control-flow
            // (`@if`/`@for`/... in `.html`) that tree-sitter-html cannot model is
            // a known framework limitation; symbols are extracted best-effort, so
            // surface it as parsed rather than a bare "partial" in the
            // file-context envelope (the report's actual repro surface).
            if crate::live_index::query::is_expected_framework_partial_parse(file) {
                return "parsed";
            }
            // SF-003: a partial parse caused only by the tree-sitter-typescript
            // 0.23.2 import-type-array grammar limitation is valid TypeScript;
            // surface it as parsed rather than partial in the file-context
            // envelope (the report's repro surface).
            if crate::parsing::is_expected_typescript_import_type_array_limitation(
                &file.language,
                &file.content,
                crate::domain::LanguageId::is_tsx_path(&file.relative_path),
            ) {
                "parsed"
            } else {
                "partial"
            }
        }
        crate::live_index::store::ParseStatus::Failed { .. } => "degraded",
    }
}

pub(crate) fn format_context_envelope(
    match_type: &str,
    source_authority: ContextSourceAuthority,
    parse_state: &str,
    completeness: &str,
    scope: impl Into<String>,
    evidence: impl Into<String>,
) -> String {
    let authority = context_source_authority_label(source_authority);
    let scope = scope.into();
    let evidence = evidence.into();
    // "Silence is the happy path" (see format_search_envelope): collapse the four
    // baseline status lines on a fully-trusted result; keep the full six-line
    // envelope when anything deviates so degraded/stale results stay loud.
    if authority == "current index" && parse_state == "parsed" && completeness.starts_with("full") {
        format!(
            "Trust: {match_type} | {authority} | {parse_state} | {completeness}\nScope: {scope}\nEvidence: {evidence}"
        )
    } else {
        format!(
            "Match type: {match_type}\nSource authority: {authority}\nParse state: {parse_state}\nCompleteness: {completeness}\nScope: {scope}\nEvidence: {evidence}"
        )
    }
}

pub(crate) fn append_parse_status_lines(
    lines: &mut Vec<String>,
    file: &crate::live_index::store::IndexedFile,
) {
    match &file.parse_status {
        crate::live_index::store::ParseStatus::Parsed => {}
        crate::live_index::store::ParseStatus::PartialParse { warning } => {
            // SF-004: suppress the partial-parse diagnostic when the only cause
            // is Angular template control-flow (`@if`/`@for`/... in `.html`) that
            // tree-sitter-html cannot model. Surface a non-alarming framework note
            // instead so the file-context envelope does not flag a known
            // framework-template limitation as a defect (the report's repro tool).
            if crate::live_index::query::is_expected_framework_partial_parse(file) {
                lines.push(
                    "Parse status: ok (framework limitation: Angular template control-flow \
                     is not supported by tree-sitter-html; symbols extracted best-effort)"
                        .to_string(),
                );
                return;
            }
            // SF-003: suppress the partial-parse diagnostic when the only cause
            // is the known tree-sitter-typescript 0.23.2 import-type-array
            // grammar limitation (valid TypeScript). Surface a non-alarming note
            // instead so the file-context envelope does not flag valid source.
            if crate::parsing::is_expected_typescript_import_type_array_limitation(
                &file.language,
                &file.content,
                crate::domain::LanguageId::is_tsx_path(&file.relative_path),
            ) {
                lines.push(
                    "Parse status: ok (parser limitation: tree-sitter-typescript 0.23.2 \
                     mis-parses an import-type followed by `[]`; source is valid TypeScript)"
                        .to_string(),
                );
                return;
            }
            lines.push("Parse status: partial".to_string());
            if let Some(diagnostic) = &file.parse_diagnostic {
                lines.push(format!("Diagnostic: {}", diagnostic.summary()));
                if let Some((start, end)) = diagnostic.byte_span {
                    lines.push(format!("Byte span: {start}..{end}"));
                }
            } else {
                lines.push(format!("Diagnostic: {warning}"));
            }
        }
        crate::live_index::store::ParseStatus::Failed { error } => {
            lines.push("Parse status: failed".to_string());
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
}

pub(crate) struct OutlineProjectionParams {
    pub path: String,
    pub max_tokens: Option<u64>,
    pub sections: Option<Vec<String>>,
}
pub(crate) fn outline_text_for_generation(
    published: &crate::live_index::PublishedGeneration,
    params: &OutlineProjectionParams,
    include_savings_footer: bool,
    source_authority: ContextSourceAuthority,
) -> Option<(String, u64, u64)> {
    let guard = published.live.as_ref();

    // Return 404 for non-indexed files.
    let file = guard.get_file(&params.path)?;

    let file_bytes = file.byte_len;
    let language = format!("{:?}", file.language);
    let parse_state = parse_state_label(file);

    let include_section = |name: &str| -> bool {
        match &params.sections {
            None => true,
            Some(list) => list.iter().any(|s| s.eq_ignore_ascii_case(name)),
        }
    };
    let include_consumers = include_section("consumers");
    let include_references = include_section("references");

    // Build symbol outline lines.
    let mut body_lines: Vec<String> = Vec::new();
    body_lines.push(format!(
        "── {} ({} symbols, {}) ──",
        params.path,
        file.symbols.len(),
        language
    ));
    append_parse_status_lines(&mut body_lines, file);

    // Surface section validation warnings in the output.
    if let Some(ref section_list) = params.sections {
        let valid = ["outline", "imports", "consumers", "references", "git"];
        let unknown: Vec<&str> = section_list
            .iter()
            .filter(|s| !valid.iter().any(|v| s.eq_ignore_ascii_case(v)))
            .map(|s| s.as_str())
            .collect();
        if !unknown.is_empty() {
            body_lines.push(format!(
                "Warning: unknown section(s): {}. Valid: {}.",
                unknown.join(", "),
                valid.join(", ")
            ));
        }
    }

    let mut budget_omissions = false;
    if include_section("outline") {
        let symbol_cap = params
            .max_tokens
            .map(|tokens| ((tokens as usize).saturating_div(12)).clamp(25, 500));
        let symbols_to_render = symbol_cap
            .map(|cap| cap.min(file.symbols.len()))
            .unwrap_or(file.symbols.len());
        for sym in file.symbols.iter().take(symbols_to_render) {
            let indent = "  ".repeat(sym.depth as usize);
            let kind_str = sym.kind.to_string();
            // Strip redundant kind prefix from name (e.g., impl blocks named "impl Foo").
            let display_name = if sym.name.starts_with(&format!("{} ", kind_str)) {
                &sym.name[kind_str.len() + 1..]
            } else {
                &sym.name[..]
            };
            body_lines.push(format!(
                "{}  {:<10} {}  L{}-{}",
                indent,
                kind_str,
                display_name,
                sym.line_range.0 + 1,
                sym.line_range.1 + 1,
            ));
        }
        if symbols_to_render < file.symbols.len() {
            budget_omissions = true;
            body_lines.push(format!(
                "  ...omitted {} symbols due to budget; pass a larger max_tokens or request get_file_content(start_line,end_line) for exact text",
                file.symbols.len() - symbols_to_render
            ));
        }
    }

    // Build "Imports from" section.
    // Group import references by source (qualified_name or name), count per source.
    if include_section("imports") {
        let mut import_sources: std::collections::HashMap<&str, usize> =
            std::collections::HashMap::new();
        for reference in &file.references {
            if reference.kind == ReferenceKind::Import {
                let source = reference
                    .qualified_name
                    .as_deref()
                    .unwrap_or(&reference.name);
                *import_sources.entry(source).or_insert(0) += 1;
            }
        }
        if !import_sources.is_empty() {
            let mut sorted: Vec<_> = import_sources.into_iter().collect();
            sorted.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
            body_lines.push(String::new());
            body_lines.push(format!("Imports from ({} sources):", sorted.len()));
            for (source, count) in sorted.iter().take(10) {
                body_lines.push(format!("  {} ({} symbols)", source, count));
            }
            if sorted.len() > 10 {
                body_lines.push(format!("  ...and {} more", sorted.len() - 10));
            }
        }
    }

    // Build "Used by" section.
    // Group dependents by consuming file, count references per consumer.
    let attributed_dependents = if include_consumers || include_references {
        guard.find_dependents_for_file(&params.path)
    } else {
        Vec::new()
    };
    if include_consumers {
        let mut consumers: std::collections::HashMap<&str, usize> =
            std::collections::HashMap::new();
        for (file_path, _) in &attributed_dependents {
            *consumers.entry(*file_path).or_insert(0) += 1;
        }
        if !consumers.is_empty() {
            let mut sorted: Vec<_> = consumers.into_iter().collect();
            sorted.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
            body_lines.push(String::new());
            body_lines.push(format!("Used by ({} files):", sorted.len()));
            for (consumer, count) in sorted.iter().take(10) {
                body_lines.push(format!("  {} ({} refs)", consumer, count));
            }
            if sorted.len() > 10 {
                body_lines.push(format!("  ...and {} more", sorted.len() - 10));
            }
        }
    }

    // Build "Key references" section.
    // Rank symbols by caller count descending, take top 5, show up to 3 callers each.
    if include_references {
        let mut symbol_callers: Vec<(String, Vec<(String, u32)>)> = Vec::new();

        for sym in &file.symbols {
            let external_callers: Vec<(String, u32)> = attributed_dependents
                .iter()
                .filter(|(_, reference)| {
                    reference.kind != ReferenceKind::Import && reference.name == sym.name
                })
                .map(|(fp, r)| (fp.to_string(), r.line_range.0 + 1))
                .take(3)
                .collect();

            if !external_callers.is_empty() {
                symbol_callers.push((sym.name.clone(), external_callers));
            }
        }

        // Sort by caller count descending, take top 5.
        symbol_callers.sort_by_key(|(_, callers)| std::cmp::Reverse(callers.len()));
        symbol_callers.truncate(5);

        if !symbol_callers.is_empty() {
            body_lines.push(String::new());
            body_lines.push("Key references:".to_string());
            for (sym_name, callers) in &symbol_callers {
                body_lines.push(format!("  {}()", sym_name));
                for (caller_file, caller_line) in callers {
                    body_lines.push(format!("    {}  line {}", caller_file, caller_line));
                }
            }
        }
    }

    // Build "Git activity" section from temporal intelligence.
    if include_section("git") {
        use crate::live_index::git_temporal::{
            GitTemporalState, churn_bar, churn_label, relative_time,
        };
        let temporal = &published.code_signals.temporal;
        if temporal.state == GitTemporalState::Ready
            && let Some(history) = temporal.files.get(&params.path)
        {
            body_lines.push(String::new());
            body_lines.push(format!(
                "Git activity:  {} {:.2} ({})    {} commits, last {}",
                churn_bar(history.churn_score),
                history.churn_score,
                churn_label(history.churn_score),
                history.commit_count,
                relative_time(history.last_commit.days_ago),
            ));
            body_lines.push(format!(
                "  Last:  {} \"{}\" ({}, {})",
                history.last_commit.hash,
                history.last_commit.message_head,
                history.last_commit.author,
                history.last_commit.timestamp,
            ));
            if !history.contributors.is_empty() {
                let owners: Vec<String> = history
                    .contributors
                    .iter()
                    .map(|c| format!("{} {:.0}%", c.author, c.percentage))
                    .collect();
                body_lines.push(format!("  Owners: {}", owners.join(", ")));
            }
            if !history.co_changes.is_empty() {
                body_lines.push("  Co-changes:".to_string());
                for entry in &history.co_changes {
                    body_lines.push(format!(
                        "    {}  ({:.2} coupling, {} shared commits)",
                        entry.path, entry.coupling_score, entry.shared_commits,
                    ));
                }
            }
        }
    }

    // Apply budget enforcement.
    // Hook path: default 200 tokens (800 bytes) for compact hook output.
    // Tool path: no cap unless explicitly requested — section filtering
    // must be visible, not masked by a tiny default budget.
    let max_bytes = match params.max_tokens {
        Some(n) => n * 4,
        None if include_savings_footer => 200 * 4, // hook path: compact
        None => 0,                                 // tool path: unlimited (0 = no cap)
    };
    let (body_text, remaining) = build_with_budget(&body_lines, max_bytes);
    let completeness = if remaining > 0 || budget_omissions {
        "budget-limited"
    } else {
        "full"
    };
    let scope = match &params.sections {
        Some(sections) if !sections.is_empty() => {
            format!("path `{}`; sections {}", params.path, sections.join(", "))
        }
        _ => format!("path `{}`; all sections", params.path),
    };
    let envelope = format_context_envelope(
        "exact",
        source_authority,
        parse_state,
        completeness,
        scope,
        format!("file anchor `{}`", params.path),
    );
    let mut text = format!("{envelope}\n\n{body_text}");

    let output_bytes = text.len() as u64;
    if include_savings_footer {
        text.push_str(&compact_savings_footer(
            output_bytes as usize,
            file_bytes as usize,
        ));
    }

    Some((text, file_bytes, output_bytes))
}

use super::file_read::{
    COMPETENT_READ_WINDOW_LINES, SMALL_FILE_CHAR_THRESHOLD, competent_manual_baseline_chars,
    estimate_tokens_from_chars,
};

pub const SMALL_FILE_LINE_THRESHOLD: usize = 50;

pub fn indexed_file_line_count(content: &[u8]) -> usize {
    if content.is_empty() {
        return 0;
    }
    content.iter().filter(|&&b| b == b'\n').count() + 1
}

pub fn is_small_indexed_file(content_len: usize, line_count: usize) -> bool {
    content_len < SMALL_FILE_CHAR_THRESHOLD || line_count <= SMALL_FILE_LINE_THRESHOLD
}

pub fn saved_tokens_whole_file(response_chars: usize, raw_chars: usize) -> u64 {
    if raw_chars <= response_chars || raw_chars < SMALL_FILE_CHAR_THRESHOLD {
        return 0;
    }
    estimate_tokens_from_chars(raw_chars.saturating_sub(response_chars))
}

pub fn saved_tokens_vs_competent_manual(response_chars: usize, raw_chars: usize) -> u64 {
    let baseline = competent_manual_baseline_chars(raw_chars);
    if baseline <= response_chars {
        return 0;
    }
    estimate_tokens_from_chars(baseline.saturating_sub(response_chars))
}

pub fn compact_savings_footer(response_chars: usize, raw_chars: usize) -> String {
    if raw_chars <= response_chars || raw_chars < SMALL_FILE_CHAR_THRESHOLD {
        return String::new();
    }
    let whole_saved = saved_tokens_whole_file(response_chars, raw_chars);
    let window_saved = saved_tokens_vs_competent_manual(response_chars, raw_chars);
    if whole_saved < 50 && window_saved < 10 {
        return String::new();
    }
    let mut parts = Vec::new();
    if whole_saved >= 50 {
        parts.push(format!("~{whole_saved} tokens vs whole-file read"));
    }
    if window_saved >= 10 {
        parts.push(format!(
            "~{window_saved} tokens vs ~{COMPETENT_READ_WINDOW_LINES}-line windowed read (competent-manual baseline)"
        ));
    }
    if parts.is_empty() {
        return String::new();
    }
    format!("\n\n{}", parts.join("; "))
}

pub fn collapse_large_test_modules(
    text: String,
    include_tests: bool,
    sections: Option<&[String]>,
) -> String {
    if include_tests || outline_explicitly_requested(sections) {
        return text;
    }

    let lines: Vec<&str> = text.lines().collect();
    let mut collapsed = Vec::with_capacity(lines.len());
    let mut index = 0;

    while index < lines.len() {
        let line = lines[index];
        let Some((module_indent, module_name)) = parse_test_module_outline_line(line) else {
            collapsed.push(line.to_string());
            index += 1;
            continue;
        };

        let mut cursor = index + 1;
        let mut test_function_count = 0usize;
        while cursor < lines.len() {
            let child = lines[cursor];
            if child.trim().is_empty() || leading_space_count(child) <= module_indent {
                break;
            }
            if child.trim_start().starts_with("fn") {
                test_function_count += 1;
            }
            cursor += 1;
        }

        if test_function_count > 100 {
            collapsed.push(line.to_string());
            collapsed.push(format!(
                "{}  ... test module `{}` collapsed: {} functions not shown (pass include_tests=true to expand)",
                " ".repeat(module_indent),
                module_name,
                test_function_count
            ));
            index = cursor;
        } else {
            collapsed.push(line.to_string());
            index += 1;
        }
    }

    let mut result = collapsed.join("\n");
    if text.ends_with('\n') {
        result.push('\n');
    }
    result
}

fn outline_explicitly_requested(sections: Option<&[String]>) -> bool {
    sections
        .map(|items| {
            items
                .iter()
                .any(|section| section.eq_ignore_ascii_case("outline"))
        })
        .unwrap_or(false)
}

fn parse_test_module_outline_line(line: &str) -> Option<(usize, &str)> {
    let trimmed = line.trim_start();
    let mut parts = trimmed.split_whitespace();
    if parts.next()? != "mod" {
        return None;
    }
    let name = parts.next()?;
    if name == "tests" || name.ends_with("_tests") || name.ends_with("Tests") {
        Some((leading_space_count(line), name))
    } else {
        None
    }
}

fn leading_space_count(line: &str) -> usize {
    line.bytes().take_while(|byte| *byte == b' ').count()
}

pub(crate) fn withheld_listing(
    unverified: &std::collections::BTreeMap<String, String>,
    scope: Option<&str>,
    limit: usize,
) -> Option<(usize, String)> {
    let in_scope: Vec<&str> = unverified
        .keys()
        .map(String::as_str)
        .filter(|path| {
            // Whole path segments: `src` covers `src/a.rs`, not `src2/a.rs`.
            scope.is_none_or(|scope| {
                path.strip_prefix(scope)
                    .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
            })
        })
        .collect();
    if in_scope.is_empty() {
        return None;
    }
    let mut listed = in_scope
        .iter()
        .take(limit)
        .copied()
        .collect::<Vec<_>>()
        .join(", ");
    if in_scope.len() > limit {
        listed.push_str(&format!(" (+{} more)", in_scope.len() - limit));
    }
    Some((in_scope.len(), listed))
}

/// Appended to a project-wide answer when the snapshot verify withheld files
/// inside its scope: those files were not searched, so the answer may be
/// missing hits from them.
pub fn withheld_not_searched_note(
    unverified: &std::collections::BTreeMap<String, String>,
    scope: Option<&str>,
) -> Option<String> {
    let (count, listed) = withheld_listing(unverified, scope, 5)?;
    Some(format!(
        "Note: {count} files withheld as unverified since restore were not searched: {listed}. \
         A successful re-read releases each one, and index_folder rebuilds the project from source."
    ))
}
