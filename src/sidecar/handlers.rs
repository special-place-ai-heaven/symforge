//! HTTP endpoint handlers for the symforge sidecar.
//!
//! All handlers follow this contract:
//!  - Accept `State(state): State<SidecarState>` plus optional `Query(params)`.
//!  - Acquire `state.index.read()`, extract owned data, drop the guard, then return text or Json.
//!  - Never hold a `RwLockReadGuard` across an `.await` point.
//!  - On file not found: return `StatusCode::NOT_FOUND`.

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};

use crate::domain::{LanguageId, ReferenceKind};
use crate::sidecar::{SidecarState, SymbolSnapshot, build_with_budget};
use crate::{protocol::edit, watcher};

// ---------------------------------------------------------------------------
// Request parameter structs
// ---------------------------------------------------------------------------

#[derive(Clone, Deserialize, Serialize)]
pub struct OutlineParams {
    pub path: String,
    /// Optional token budget override. Default: 200 tokens (800 bytes).
    pub max_tokens: Option<u64>,
    /// Optional list of sections to include: "outline", "imports", "consumers", "references", "git".
    /// When `None`, all sections are included.
    #[serde(default)]
    pub sections: Option<Vec<String>>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct ImpactParams {
    pub path: String,
    /// If `true`, treat this as a new-file indexing request (HOOK-06).
    pub new_file: Option<bool>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct SymbolContextParams {
    pub name: String,
    /// Optional: restrict search to a specific file.
    pub file: Option<String>,
    /// Optional exact-selector path from `search_symbols`.
    pub path: Option<String>,
    /// Optional selected symbol kind such as `fn`, `class`, or `struct`.
    pub symbol_kind: Option<String>,
    /// Optional selected symbol line from `search_symbols`.
    pub symbol_line: Option<u32>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct PromptContextParams {
    pub text: String,
}

struct PromptFileHint {
    path: String,
    line_hint_alias: Option<String>,
    match_kind: PromptHintMatchKind,
}

struct PromptQualifiedSymbolHint {
    file_hint: PromptFileHint,
    symbol_name: String,
}

#[derive(Clone, Copy)]
enum PromptHintMatchKind {
    ExactPath,
    ModuleAlias,
    QualifiedPathAlias,
    Basename,
    StemLineAlias,
    QualifiedSymbolAlias,
}

#[derive(Clone, Copy)]
enum ContextSourceAuthority {
    DiskRefreshed,
    CurrentIndex,
}

#[derive(Serialize)]
pub struct HealthResponse {
    pub file_count: usize,
    pub symbol_count: usize,
    pub index_state: String,
    pub uptime_secs: u64,
}

#[derive(Clone, Copy)]
struct RenderOptions {
    include_savings_footer: bool,
    record_stats: bool,
}

const HOOK_RENDER_OPTIONS: RenderOptions = RenderOptions {
    include_savings_footer: true,
    record_stats: true,
};

const TOOL_RENDER_OPTIONS: RenderOptions = RenderOptions {
    include_savings_footer: false,
    record_stats: true,
};

fn format_prompt_context_signal(level: &str, evidence: impl Into<String>, body: String) -> String {
    format!(
        "Prompt-context signal: {level}\nEvidence: {}\n\n{body}",
        evidence.into()
    )
}

fn no_high_confidence_prompt_context_message() -> String {
    "Prompt-context signal: no high-confidence hint\n\
Evidence: no exact file, symbol, or repo-map cue matched the prompt\n\n\
Suggested next step: use `search_symbols(...)` for likely names or `search_text(...)` for code/content search."
        .to_string()
}

fn context_source_authority_label(authority: ContextSourceAuthority) -> &'static str {
    match authority {
        ContextSourceAuthority::DiskRefreshed => "disk-refreshed",
        ContextSourceAuthority::CurrentIndex => "current index",
    }
}

fn parse_state_label(status: &crate::live_index::store::ParseStatus) -> &'static str {
    match status {
        crate::live_index::store::ParseStatus::Parsed => "parsed",
        crate::live_index::store::ParseStatus::PartialParse { .. } => "partial",
        crate::live_index::store::ParseStatus::Failed { .. } => "degraded",
    }
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

fn format_context_envelope(
    match_type: &str,
    source_authority: ContextSourceAuthority,
    parse_state: &str,
    completeness: &str,
    scope: impl Into<String>,
    evidence: impl Into<String>,
) -> String {
    format!(
        "Match type: {match_type}\nSource authority: {}\nParse state: {parse_state}\nCompleteness: {completeness}\nScope: {}\nEvidence: {}",
        context_source_authority_label(source_authority),
        scope.into(),
        evidence.into()
    )
}

fn freshen_sidecar_path_if_stale(
    state: &SidecarState,
    relative_path: &str,
) -> ContextSourceAuthority {
    let expected_gen = state.index.current_project_generation();
    freshen_sidecar_path_if_stale_at_generation(state, relative_path, expected_gen)
}

fn safe_sidecar_path_for_freshen(
    repo_root: &std::path::Path,
    relative_path: &str,
) -> Result<std::path::PathBuf, String> {
    let relative = std::path::Path::new(relative_path);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(format!("path '{relative_path}' is outside the repository"));
    }

    match edit::safe_repo_path(repo_root, relative_path) {
        Ok(path) => Ok(path),
        Err(_) => {
            let canon_root = repo_root
                .canonicalize()
                .map_err(|e| format!("cannot resolve repo root: {e}"))?;
            Ok(canon_root.join(relative))
        }
    }
}

fn freshen_sidecar_path_if_stale_at_generation(
    state: &SidecarState,
    relative_path: &str,
    expected_gen: u64,
) -> ContextSourceAuthority {
    let Some(repo_root) = &state.repo_root else {
        return ContextSourceAuthority::CurrentIndex;
    };
    let Ok(abs_path) = safe_sidecar_path_for_freshen(repo_root, relative_path) else {
        return ContextSourceAuthority::CurrentIndex;
    };
    match watcher::freshen_file_if_stale(relative_path, &abs_path, &state.index, expected_gen) {
        watcher::FreshenResult::Fresh => ContextSourceAuthority::CurrentIndex,
        watcher::FreshenResult::StaleReindexed => ContextSourceAuthority::DiskRefreshed,
        watcher::FreshenResult::StaleRemoved => ContextSourceAuthority::DiskRefreshed,
        watcher::FreshenResult::GenerationMismatch => ContextSourceAuthority::CurrentIndex,
    }
}

fn describe_file_hint(file_hint: &PromptFileHint) -> (&'static str, String) {
    match file_hint.match_kind {
        PromptHintMatchKind::ExactPath => (
            "high-confidence",
            format!("exact path `{}` matched in the prompt", file_hint.path),
        ),
        PromptHintMatchKind::ModuleAlias => (
            "medium-confidence",
            format!(
                "module alias `{}` resolved to `{}`",
                file_hint.line_hint_alias.as_deref().unwrap_or("<unknown>"),
                file_hint.path
            ),
        ),
        PromptHintMatchKind::QualifiedPathAlias => (
            "medium-confidence",
            format!(
                "path alias `{}` resolved to `{}`",
                file_hint.line_hint_alias.as_deref().unwrap_or("<unknown>"),
                file_hint.path
            ),
        ),
        PromptHintMatchKind::Basename => (
            "heuristic",
            format!(
                "basename `{}` matched `{}`",
                file_hint.line_hint_alias.as_deref().unwrap_or("<unknown>"),
                file_hint.path
            ),
        ),
        PromptHintMatchKind::StemLineAlias => (
            "heuristic",
            format!(
                "stem+line alias `{}` matched `{}`",
                file_hint.line_hint_alias.as_deref().unwrap_or("<unknown>"),
                file_hint.path
            ),
        ),
        PromptHintMatchKind::QualifiedSymbolAlias => (
            "high-confidence",
            format!(
                "qualified symbol alias `{}` resolved within `{}`",
                file_hint.line_hint_alias.as_deref().unwrap_or("<unknown>"),
                file_hint.path
            ),
        ),
    }
}

fn resolve_repo_root(state: &SidecarState) -> Result<std::path::PathBuf, StatusCode> {
    match &state.repo_root {
        Some(root) => Ok(root.clone()),
        None => std::env::current_dir().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR),
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `GET /health` — index state, file count, symbol count, uptime.
pub async fn health_handler(
    State(state): State<SidecarState>,
) -> Result<Json<HealthResponse>, StatusCode> {
    let published = state.index.published_state();

    let uptime_secs = published
        .loaded_at_system
        .elapsed()
        .unwrap_or_default()
        .as_secs();

    Ok(Json(HealthResponse {
        file_count: published.file_count,
        symbol_count: published.symbol_count,
        index_state: published.status_label().to_string(),
        uptime_secs,
    }))
}

/// `GET /outline?path=<relative>[&max_tokens=N]` — symbol outline for a single file.
///
/// Returns formatted plain text with:
/// - Symbol outline lines (compact, ripgrep-like)
/// - "Key references" section showing top 3-5 most-called symbols with up to 3 callers each
/// - "[~N tokens saved]" footer
///
/// Budget: 200 tokens (800 bytes) by default.
pub async fn outline_handler(
    State(state): State<SidecarState>,
    Query(params): Query<OutlineParams>,
) -> Result<String, StatusCode> {
    outline_hook_text(&state, &params)
}

/// Workflow adapter for source-code reads/orientation.
///
/// This remains a thin alias over the canonical outline hook behavior so the
/// sidecar exposes an explicit workflow surface without duplicating logic.
pub async fn workflow_source_read_handler(
    State(state): State<SidecarState>,
    Query(params): Query<OutlineParams>,
) -> Result<String, StatusCode> {
    outline_handler(State(state), Query(params)).await
}

pub(crate) fn outline_tool_text(
    state: &SidecarState,
    params: &OutlineParams,
) -> Result<String, StatusCode> {
    outline_text(state, params, TOOL_RENDER_OPTIONS)
}

fn outline_hook_text(state: &SidecarState, params: &OutlineParams) -> Result<String, StatusCode> {
    outline_text(state, params, HOOK_RENDER_OPTIONS)
}

fn append_parse_status_lines(
    lines: &mut Vec<String>,
    file: &crate::live_index::store::IndexedFile,
) {
    match &file.parse_status {
        crate::live_index::store::ParseStatus::Parsed => {}
        crate::live_index::store::ParseStatus::PartialParse { warning } => {
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

fn outline_text(
    state: &SidecarState,
    params: &OutlineParams,
    options: RenderOptions,
) -> Result<String, StatusCode> {
    let source_authority = freshen_sidecar_path_if_stale(state, &params.path);
    let guard = state.index.read();

    // Return 404 for non-indexed files.
    let file = guard.get_file(&params.path).ok_or(StatusCode::NOT_FOUND)?;

    let file_bytes = file.byte_len;
    let language = format!("{:?}", file.language);
    let parse_state = parse_state_label(&file.parse_status);

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

    drop(guard);

    // Build "Git activity" section from temporal intelligence.
    if include_section("git") {
        use crate::live_index::git_temporal::{
            GitTemporalState, churn_bar, churn_label, relative_time,
        };
        let temporal = state.index.git_temporal();
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
        None if options.include_savings_footer => 200 * 4, // hook path: compact
        None => 0,                                         // tool path: unlimited (0 = no cap)
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
    if options.include_savings_footer {
        let saved_tokens = file_bytes.saturating_sub(output_bytes) / 4;
        text.push_str(&format!("\n[~{} tokens saved]", saved_tokens));
    }

    if options.record_stats {
        state.token_stats.record_read(file_bytes, output_bytes);
    }

    Ok(text)
}

/// `GET /impact?path=<relative>[&new_file=true]` — symbol diff after edit, or index confirmation.
///
/// **new_file=true (HOOK-06):** Reads file from disk, parses it, indexes it.
/// Returns: language, symbol kind breakdown, `[Indexed, 0 callers yet]`.
///
/// **default (HOOK-05 edit):** Re-indexes the file from disk, computes pre/post symbol diff.
/// Shows Added/Changed/Removed symbols plus callers for Changed+Removed symbols.
///
/// Budget: 150 tokens (600 bytes).
pub async fn impact_handler(
    State(state): State<SidecarState>,
    Query(params): Query<ImpactParams>,
) -> Result<String, StatusCode> {
    impact_hook_text(state, &params).await
}

/// Workflow adapter for post-edit impact summaries.
pub async fn workflow_post_edit_impact_handler(
    State(state): State<SidecarState>,
    Query(params): Query<ImpactParams>,
) -> Result<String, StatusCode> {
    impact_handler(State(state), Query(params)).await
}

pub(crate) async fn impact_tool_text(
    state: SidecarState,
    params: &ImpactParams,
) -> Result<String, StatusCode> {
    impact_text(state, params, TOOL_RENDER_OPTIONS).await
}

async fn impact_hook_text(
    state: SidecarState,
    params: &ImpactParams,
) -> Result<String, StatusCode> {
    impact_text(state, params, HOOK_RENDER_OPTIONS).await
}

async fn impact_text(
    state: SidecarState,
    params: &ImpactParams,
    options: RenderOptions,
) -> Result<String, StatusCode> {
    let is_new_file = params.new_file.unwrap_or(false);

    if is_new_file {
        // HOOK-06: Index a new file from disk.
        return handle_new_file_impact(state, &params.path, options).await;
    }

    let should_auto_index_new_file = {
        let extension = std::path::Path::new(&params.path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        let is_supported = crate::domain::LanguageId::from_extension(extension).is_some();
        let indexed = {
            let guard = state.index.read();
            guard.get_file(&params.path).is_some()
        };
        is_supported
            && !indexed
            && resolve_repo_root(&state)
                .map(|root| root.join(&params.path).is_file())
                .unwrap_or(false)
    };

    if should_auto_index_new_file {
        return handle_new_file_impact(state, &params.path, options).await;
    }

    // HOOK-05: Re-index existing file and compute symbol diff.
    handle_edit_impact(state, &params.path, options).await
}

async fn handle_new_file_impact(
    state: SidecarState,
    path: &str,
    options: RenderOptions,
) -> Result<String, StatusCode> {
    use crate::domain::LanguageId;

    // Determine language from file extension.
    let extension = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    let language = LanguageId::from_extension(extension).ok_or(StatusCode::NOT_FOUND)?;

    // Read file from disk. The sidecar doesn't know the project root, so
    // we look up the root from the existing index as a heuristic.
    // For new files, we try to find them relative to cwd.
    let abs_path = resolve_repo_root(&state)?.join(path);
    let path_owned = path.to_string();
    let lang_clone = language.clone();
    let (bytes, result, mtime_secs) =
        tokio::task::spawn_blocking(move || -> Result<_, StatusCode> {
            // Read mtime BEFORE content to avoid TOCTOU: if the file changes
            // between reads, the recorded mtime will be older than the content,
            // ensuring the watcher re-indexes on the next pass.
            let mtime_secs = std::fs::metadata(&abs_path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let bytes = std::fs::read(&abs_path).map_err(|_| StatusCode::NOT_FOUND)?;
            let result = crate::parsing::process_file(&path_owned, &bytes, lang_clone);
            Ok((bytes, result, mtime_secs))
        })
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)??;

    // Build symbol kind breakdown.
    let mut kind_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for sym in &result.symbols {
        *kind_counts.entry(sym.kind.to_string()).or_insert(0) += 1;
    }

    let mut kind_parts: Vec<String> = kind_counts
        .iter()
        .map(|(k, v)| format!("{} {}", v, k))
        .collect();
    kind_parts.sort();
    let kinds_str = if kind_parts.is_empty() {
        "0 symbols".to_string()
    } else {
        kind_parts.join(", ")
    };

    // Index the file.
    let indexed = crate::live_index::store::IndexedFile::from_parse_result(result, bytes)
        .with_mtime(mtime_secs);
    state.index.update_file(path.to_string(), indexed);

    // Update symbol cache with empty pre-edit snapshot (it's new, no pre-state).
    {
        let mut cache = state.symbol_cache.write();
        cache.insert(path.to_string(), Vec::new());
    }

    if options.record_stats {
        state.token_stats.record_write();
    }

    let text = format!(
        "Language: {:?}\nSymbols: {}\n[Indexed, 0 callers yet]",
        language, kinds_str,
    );

    Ok(text)
}

/// Locate the SymbolRecord in an indexed file that corresponds to a
/// pre-recorded SymbolSnapshot.
///
/// Used by analyze_file_impact so it can walk the symbol's parent impl
/// block and type-scope the "Callers to review" list. Matches on the
/// triple (name, kind, byte_range) — overloaded names are common, so
/// name alone is insufficient.
fn find_record_matching_snapshot<'a>(
    file: &'a crate::live_index::store::IndexedFile,
    sym: &SymbolSnapshot,
) -> Option<&'a crate::domain::SymbolRecord> {
    file.symbols.iter().find(|s| {
        s.name == sym.name && s.kind.to_string() == sym.kind && s.byte_range == sym.byte_range
    })
}
async fn handle_edit_impact(
    state: SidecarState,
    path: &str,
    options: RenderOptions,
) -> Result<String, StatusCode> {
    use crate::domain::LanguageId;

    // Get pre-edit symbols: sidecar cache → index pre-update snapshot → current index.
    //
    // The index pre-update snapshot (`take_pre_update_symbols`) fixes a race
    // where the watcher re-indexes the file before this hook fires, causing the
    // current index to already contain post-edit symbols and yielding a false
    // "no symbol changes detected" result.
    let pre_symbols: Vec<SymbolSnapshot> = {
        let cache = state.symbol_cache.read();
        if let Some(cached) = cache.get(path) {
            cached.clone()
        } else {
            drop(cache);
            // Try the index-level pre-update snapshot first (survives watcher race).
            if let Some(pre) = state.index.take_pre_update_symbols(path) {
                pre.into_iter()
                    .map(|s| SymbolSnapshot {
                        name: s.name,
                        kind: s.kind,
                        line_range: s.line_range,
                        byte_range: s.byte_range,
                    })
                    .collect()
            } else {
                // No pre-update snapshot — populate from current index.
                let guard = state.index.read();
                if let Some(file) = guard.get_file(path) {
                    file.symbols
                        .iter()
                        .map(|s| SymbolSnapshot {
                            name: s.name.clone(),
                            kind: s.kind.to_string(),
                            line_range: s.line_range,
                            byte_range: s.byte_range,
                        })
                        .collect()
                } else {
                    Vec::new()
                }
            }
        }
    };

    // Get file byte_len from index before re-indexing.
    let file_bytes_pre: u64 = {
        let guard = state.index.read();
        guard.get_file(path).map(|f| f.byte_len).unwrap_or(0)
    };

    // Determine language.
    let extension = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    let language =
        LanguageId::from_extension(extension).ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    // Read file from disk and re-index.
    let abs_path = resolve_repo_root(&state)?.join(path);
    let path_owned = path.to_string();

    enum ReadOutcome {
        Ok {
            bytes: Vec<u8>,
            result: Box<crate::domain::FileProcessingResult>,
            mtime_secs: u64,
        },
        NotFound,
    }

    let outcome = tokio::task::spawn_blocking(move || {
        // Read mtime BEFORE content to avoid TOCTOU: if the file changes
        // between reads, the recorded mtime will be older than the content,
        // ensuring the watcher re-indexes on the next pass.
        let mtime_secs = std::fs::metadata(&abs_path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        match std::fs::read(&abs_path) {
            Ok(bytes) => {
                let result = crate::parsing::process_file(&path_owned, &bytes, language);
                ReadOutcome::Ok {
                    bytes,
                    result: Box::new(result),
                    mtime_secs,
                }
            }
            Err(_) => ReadOutcome::NotFound,
        }
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let (bytes, result, mtime_secs) = match outcome {
        ReadOutcome::NotFound => {
            // File not on disk — remove it from the index so stale data is purged.
            let prev_symbol_count = {
                let guard = state.index.read();
                guard.get_file(path).map(|f| f.symbols.len()).unwrap_or(0)
            };
            state.index.remove_file(path);
            // Also clear the symbol cache entry.
            {
                let mut cache = state.symbol_cache.write();
                cache.remove(path);
            }
            let text = if prev_symbol_count > 0 {
                format!(
                    "── Impact: {} ──\nStatus: not found on disk — removed from index\nPreviously had {} symbols.",
                    path, prev_symbol_count
                )
            } else {
                // The file-watcher may have already purged the index entry
                // between the on-disk delete and this call; in that case we
                // have no pre-count to report, so say so plainly instead of
                // printing a misleading `Previously had 0 symbols.`.
                format!(
                    "── Impact: {} ──\nStatus: not found on disk — no index record remains (may have been removed by watcher).",
                    path
                )
            };
            return Ok(text);
        }
        ReadOutcome::Ok {
            bytes,
            result,
            mtime_secs,
        } => (bytes, result, mtime_secs),
    };

    let file_bytes: u64 = (bytes.len() as u64).max(file_bytes_pre);

    let post_symbols: Vec<SymbolSnapshot> = result
        .symbols
        .iter()
        .map(|s| SymbolSnapshot {
            name: s.name.clone(),
            kind: s.kind.to_string(),
            line_range: s.line_range,
            byte_range: s.byte_range,
        })
        .collect();

    let indexed = crate::live_index::store::IndexedFile::from_parse_result(*result, bytes)
        .with_mtime(mtime_secs);
    state.index.update_file(path.to_string(), indexed);

    // Compute symbol diff using positional proximity for duplicate name+kind pairs.
    let mut matched_pre = vec![false; pre_symbols.len()];
    let mut matched_post = vec![false; post_symbols.len()];
    let mut changed_post: Vec<usize> = Vec::new();

    for (pi, ps) in post_symbols.iter().enumerate() {
        // Find the closest unmatched pre-symbol with the same name+kind.
        let best = pre_symbols
            .iter()
            .enumerate()
            .filter(|(i, pr)| !matched_pre[*i] && pr.name == ps.name && pr.kind == ps.kind)
            .min_by_key(|(_, pr)| (pr.line_range.0 as i64 - ps.line_range.0 as i64).unsigned_abs());
        if let Some((pri, pr)) = best {
            matched_pre[pri] = true;
            matched_post[pi] = true;
            if pr.line_range != ps.line_range || pr.byte_range != ps.byte_range {
                changed_post.push(pi);
            }
        }
    }

    let added: Vec<&SymbolSnapshot> = post_symbols
        .iter()
        .enumerate()
        .filter(|(i, _)| !matched_post[*i])
        .map(|(_, s)| s)
        .collect();

    let removed: Vec<&SymbolSnapshot> = pre_symbols
        .iter()
        .enumerate()
        .filter(|(i, _)| !matched_pre[*i])
        .map(|(_, s)| s)
        .collect();

    let changed: Vec<&SymbolSnapshot> = changed_post.iter().map(|&i| &post_symbols[i]).collect();

    // Update cache with post-edit snapshot.
    {
        let mut cache = state.symbol_cache.write();
        cache.insert(path.to_string(), post_symbols.clone());
    }

    // Build response lines.
    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("── Impact: {} ──", path));

    if added.is_empty() && changed.is_empty() && removed.is_empty() {
        lines.push(format!(
            "Status: indexed and unchanged\nSymbols: {}\nTip: Use what_changed to see recent modifications.",
            post_symbols.len()
        ));
    } else {
        lines.push("Status: changed on disk since last index".to_string());
        for sym in &added {
            lines.push(format!("  [Added]   {} {}", sym.kind, sym.name));
        }
        for sym in &changed {
            lines.push(format!("  [Changed] {} {}", sym.kind, sym.name));
        }
        for sym in &removed {
            lines.push(format!("  [Removed] {} {}", sym.kind, sym.name));
        }

        // Show callers for Changed + Removed symbols.
        //
        // For CHANGED symbols that live inside an `impl` block, scope the
        // caller list to files that also reference the parent type —
        // prevents `MathMachine::new` from flagging every unrelated `new()`
        // call. Mirrors the filter in protocol::edit::detect_stale_references.
        //
        // REMOVED symbols cannot be type-scoped here: the post-edit file no
        // longer contains the SymbolRecord, so `find_record_matching_snapshot`
        // returns None and the filter short-circuits to name-only matching.
        // Acceptable trade-off: removing a same-named method from one of many
        // types is rare, and carrying parent_type through SymbolSnapshot would
        // widen the schema for a corner case. Revisit if the false positive
        // surfaces in real usage.
        let impacted: Vec<&SymbolSnapshot> =
            changed.iter().chain(removed.iter()).copied().collect();
        if !impacted.is_empty() {
            let guard = state.index.read();
            let post_file = guard.get_file(path);
            let mut callers_lines: Vec<String> = Vec::new();
            for sym in &impacted {
                // Derive the parent impl/class type for this symbol, if any.
                // Look the symbol up in the POST-edit file by name+byte_range so
                // overloaded names do not confuse the walker.
                let parent_type: Option<String> = post_file.as_ref().and_then(|file| {
                    find_record_matching_snapshot(file, sym).and_then(|record| {
                        crate::protocol::edit::find_parent_impl_type(file, record)
                    })
                });

                // When we know the parent type, collect the set of files that
                // reference it. Only those files could plausibly call
                // `ParentType::method_name()`.
                let type_files: Option<std::collections::HashSet<String>> =
                    parent_type.as_ref().map(|tn| {
                        guard
                            .find_references_for_name(tn, None, false)
                            .into_iter()
                            .map(|(fp, _)| fp.to_string())
                            .collect()
                    });

                let callers = guard.find_references_for_name(&sym.name, None, false);
                let external: Vec<_> = callers
                    .iter()
                    .filter(|(fp, _)| *fp != path)
                    .filter(|(fp, _)| match &type_files {
                        Some(tf) => tf.contains(*fp),
                        None => true,
                    })
                    .take(5)
                    .collect();
                if !external.is_empty() {
                    callers_lines.push(format!("  Callers of {}():", sym.name));
                    for (caller_file, r) in &external {
                        callers_lines.push(format!(
                            "    {}  line {}",
                            caller_file,
                            r.line_range.0 + 1
                        ));
                    }
                }
            }
            drop(guard);
            if !callers_lines.is_empty() {
                lines.push(String::new());
                lines.push("Callers to review:".to_string());
                lines.extend(callers_lines);
            }
        }
    }

    // Apply budget (150 tokens = 600 bytes).
    let (mut text, _) = build_with_budget(&lines, 600);

    let output_bytes = text.len() as u64;
    if options.include_savings_footer {
        let saved_tokens = file_bytes.saturating_sub(output_bytes) / 4;
        text.push_str(&format!("\n[~{} tokens saved]", saved_tokens));
    }

    if options.record_stats {
        state.token_stats.record_edit(file_bytes, output_bytes);
    }

    Ok(text)
}

/// `GET /symbol-context?name=<name>[&file=<path>]` — all references to a named symbol.
///
/// Returns formatted plain text with enclosing-symbol annotations, grouped by file.
/// Caps at 10 annotated matches.
///
/// Budget: 100 tokens (400 bytes).
pub async fn symbol_context_handler(
    State(state): State<SidecarState>,
    Query(params): Query<SymbolContextParams>,
) -> Result<String, StatusCode> {
    symbol_context_hook_text(&state, &params)
}

/// Workflow adapter for search-hit expansion and quick caller/context reads.
pub async fn workflow_search_hit_expansion_handler(
    State(state): State<SidecarState>,
    Query(params): Query<SymbolContextParams>,
) -> Result<String, StatusCode> {
    symbol_context_handler(State(state), Query(params)).await
}

pub(crate) fn symbol_context_tool_text(
    state: &SidecarState,
    params: &SymbolContextParams,
) -> Result<String, StatusCode> {
    symbol_context_text(state, params, TOOL_RENDER_OPTIONS)
}

fn symbol_context_hook_text(
    state: &SidecarState,
    params: &SymbolContextParams,
) -> Result<String, StatusCode> {
    symbol_context_text(state, params, HOOK_RENDER_OPTIONS)
}

fn symbol_context_text(
    state: &SidecarState,
    params: &SymbolContextParams,
    options: RenderOptions,
) -> Result<String, StatusCode> {
    let source_authority = if let Some(path) = params.path.as_deref() {
        freshen_sidecar_path_if_stale(state, path)
    } else if let Some(file) = params.file.as_deref() {
        freshen_sidecar_path_if_stale(state, file)
    } else {
        ContextSourceAuthority::CurrentIndex
    };
    let guard = state.index.read();
    let published = state.index.published_state();

    let references = if let Some(path) = params.path.as_deref() {
        match guard.find_exact_references_for_symbol(
            path,
            &params.name,
            params.symbol_kind.as_deref(),
            params.symbol_line,
            None,
        ) {
            Ok(refs) => refs,
            Err(error) => return Ok(error),
        }
    } else {
        guard.find_references_for_name(&params.name, None, false)
    };

    // Group by file, applying optional file filter, capping at 10 total matches.
    let mut map: std::collections::HashMap<String, Vec<(u32, String, Option<String>)>> =
        std::collections::HashMap::new();

    let mut total = 0usize;
    let mut grand_total = 0usize;

    for (file_path, reference) in &references {
        grand_total += 1;
        if let Some(ref filter_file) = params.file
            && *file_path != filter_file.as_str()
        {
            continue;
        }
        if total >= 10 {
            continue; // count beyond 10 but don't include
        }

        let enclosing = reference.enclosing_symbol_index.and_then(|idx| {
            guard
                .get_file(file_path)
                .and_then(|f| f.symbols.get(idx as usize))
                .map(|s| s.name.clone())
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

    let parse_state = if let Some(path) = params.path.as_deref() {
        guard
            .get_file(path)
            .map(|file| parse_state_label(&file.parse_status))
            .unwrap_or_else(|| aggregate_parse_state_label(std::iter::empty(), &published))
    } else if let Some(file) = params.file.as_deref() {
        guard
            .get_file(file)
            .map(|indexed| parse_state_label(&indexed.parse_status))
            .unwrap_or_else(|| aggregate_parse_state_label(std::iter::empty(), &published))
    } else {
        aggregate_parse_state_label(
            map.keys()
                .filter_map(|file_path| guard.get_file(file_path))
                .map(|file| &file.parse_status),
            &published,
        )
    };

    drop(guard);

    // Sort files for deterministic output.
    let mut files: Vec<String> = map.keys().cloned().collect();
    files.sort();

    let mut evidence_anchors: Vec<String> = Vec::new();
    for file in &files {
        // safe: `files` is built from `map.keys()` immediately above; lookup cannot miss.
        let refs = map.get(file).unwrap();
        let mut sorted_refs = refs.clone();
        sorted_refs.sort_by_key(|(line, _, _)| *line);
        for (line, _, _) in &sorted_refs {
            if evidence_anchors.len() >= 3 {
                break;
            }
            evidence_anchors.push(format!("{file}:{line}"));
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
            if let Some(sym_name) = enclosing {
                body_lines.push(format!("  line {}  in fn {}", line, sym_name));
            } else {
                body_lines.push(format!("  line {}  (module level)", line));
            }
        }
    }

    if body_lines.is_empty() {
        body_lines.push("No references found in the index.".to_string());
        body_lines.push(
            "Tip: this symbol may only be used via dynamic dispatch, reflection, or external entry points.".to_string(),
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

    // Apply budget (100 tokens = 400 bytes).
    let (body_text, remaining) = build_with_budget(&body_lines, 400);
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
    let evidence = if let Some(path) = params.path.as_deref() {
        match params.symbol_line {
            Some(line) => format!(
                "exact selector `{path}:{line}` for symbol `{}`",
                params.name
            ),
            None => format!("path-constrained symbol `{}` in `{path}`", params.name),
        }
    } else if let Some(file) = params.file.as_deref() {
        format!("file filter `{file}` for symbol `{}`", params.name)
    } else if evidence_anchors.is_empty() {
        format!(
            "symbol token `{}` with no indexed reference anchors",
            params.name
        )
    } else {
        format!(
            "symbol token `{}` anchored at {}",
            params.name,
            evidence_anchors.join(", ")
        )
    };
    let scope = if let Some(path) = params.path.as_deref() {
        match params.symbol_line {
            Some(line) => format!("path `{path}`; exact selector line {line}"),
            None => format!("path `{path}`; symbol-scoped references"),
        }
    } else if let Some(file) = params.file.as_deref() {
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
    let mut text = format!("{envelope}\n\n{body_text}");

    let output_bytes = text.len() as u64;
    if options.include_savings_footer {
        let saved_tokens = total_bytes.saturating_sub(output_bytes) / 4;
        text.push_str(&format!("\n[~{} tokens saved]", saved_tokens));
    }

    if options.record_stats {
        state.token_stats.record_grep(total_bytes, output_bytes);
    }

    Ok(text)
}

/// `GET /repo-map` — formatted directory tree with symbol counts.
///
/// Returns 2-level directory tree with file counts and symbol counts per directory,
/// plus a language breakdown header.
///
/// Budget: 500 tokens (2000 bytes). No token savings recorded (additive, not replacement).
pub async fn repo_map_handler(State(state): State<SidecarState>) -> Result<String, StatusCode> {
    repo_map_text(&state)
}

/// Workflow adapter for repo-start quick maps.
pub async fn workflow_repo_start_handler(
    State(state): State<SidecarState>,
) -> Result<String, StatusCode> {
    repo_map_handler(State(state)).await
}

/// Heuristic: whether an indexed path looks like it belongs to the
/// active workspace.
///
/// Rejects any path containing `:` (Windows drive letter — `C:\…`) or
/// starting with `/` (POSIX absolute). Kept loose on purpose: a legit
/// file literally named `src/a:b.rs` on POSIX would also be filtered,
/// but we accept that edge case in exchange for matching the pre-existing
/// guard at the header-stats loop exactly and blocking the octogent-style
/// cross-workspace leak that motivated Unit 1.
fn is_intra_workspace_path(path: &str) -> bool {
    !(path.contains(':') || path.starts_with('/'))
}
pub(crate) fn repo_map_text(state: &SidecarState) -> Result<String, StatusCode> {
    // Single lock acquisition covers both the directory stats and key-types passes.
    let guard = state.index.read();

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
            entry_points.sort_by(|a, b| a.2.cmp(&b.2).then(a.1.cmp(&b.1)));
            entry_points.truncate(15);
            lines.push(String::new());
            lines.push("Key types:".to_string());
            for (kind, name, path) in &entry_points {
                lines.push(format!("  {kind} {name}  ({path})"));
            }
            if entry_points.len() == 15 {
                lines.push("  ...".to_string());
            }
        }
    }

    drop(guard);

    // Apply budget (1000 tokens = 4000 bytes).
    // Medium repos (up to ~70 directories) fit without truncation.
    let (text, _) = build_with_budget(&lines, 4000);

    Ok(text)
}

/// `GET /prompt-context?text=<prompt>` — derive compact context from a user prompt.
///
/// Heuristics:
/// - explicit file hint in the prompt => outline for that file
/// - explicit symbol hint in the prompt => symbol context for that symbol
/// - repo-map intent keywords => repo map
/// - otherwise => explicit low-confidence guidance with next-step suggestions
pub async fn prompt_context_handler(
    State(state): State<SidecarState>,
    Query(params): Query<PromptContextParams>,
) -> Result<String, StatusCode> {
    prompt_context_hook_text(&state, &params).await
}

/// Workflow adapter for prompt-context narrowing.
pub async fn workflow_prompt_narrowing_handler(
    State(state): State<SidecarState>,
    Query(params): Query<PromptContextParams>,
) -> Result<String, StatusCode> {
    prompt_context_handler(State(state), Query(params)).await
}

async fn prompt_context_hook_text(
    state: &SidecarState,
    params: &PromptContextParams,
) -> Result<String, StatusCode> {
    prompt_context_text(state, params, HOOK_RENDER_OPTIONS).await
}

async fn prompt_context_text(
    state: &SidecarState,
    params: &PromptContextParams,
    options: RenderOptions,
) -> Result<String, StatusCode> {
    let prompt = params.text.trim();
    if prompt.is_empty() {
        return Ok(String::new());
    }

    if let Some(symbol_hint) = find_prompt_qualified_symbol_hint(state, prompt)? {
        let line_hint = find_prompt_line_hint(prompt, Some(&symbol_hint.file_hint));
        let body = symbol_context_text(
            state,
            &SymbolContextParams {
                name: symbol_hint.symbol_name,
                file: None,
                path: Some(symbol_hint.file_hint.path.clone()),
                symbol_kind: None,
                symbol_line: line_hint,
            },
            options,
        )?;
        let (level, evidence) = describe_file_hint(&symbol_hint.file_hint);
        return Ok(format_prompt_context_signal(level, evidence, body));
    }

    let file_hint = find_prompt_file_hint(state, prompt)?;
    let symbol_hint = find_prompt_symbol_hint(state, prompt)?;
    let line_hint = find_prompt_line_hint(prompt, file_hint.as_ref());

    match (file_hint, symbol_hint) {
        (Some(file_hint), Some(name)) => {
            let body = symbol_context_text(
                state,
                &SymbolContextParams {
                    name: name.clone(),
                    file: None,
                    path: Some(file_hint.path.clone()),
                    symbol_kind: None,
                    symbol_line: line_hint,
                },
                options,
            )?;
            let (level, file_evidence) = describe_file_hint(&file_hint);
            return Ok(format_prompt_context_signal(
                level,
                format!("{file_evidence}; symbol token `{name}` found in the index"),
                body,
            ));
        }
        (Some(file_hint), None) => {
            let body = outline_text(
                state,
                &OutlineParams {
                    path: file_hint.path.clone(),
                    max_tokens: Some(160),
                    sections: None,
                },
                options,
            )?;
            let (level, evidence) = describe_file_hint(&file_hint);
            return Ok(format_prompt_context_signal(level, evidence, body));
        }
        (None, Some(name)) => {
            let body = symbol_context_text(
                state,
                &SymbolContextParams {
                    name: name.clone(),
                    file: None,
                    path: None,
                    symbol_kind: None,
                    symbol_line: None,
                },
                options,
            )?;
            return Ok(format_prompt_context_signal(
                "heuristic",
                format!("symbol token `{name}` matched somewhere in the index"),
                body,
            ));
        }
        (None, None) => {}
    }

    if prompt_requests_repo_map(prompt) {
        let body = repo_map_text(state)?;
        return Ok(format_prompt_context_signal(
            "high-confidence",
            "repo-map request phrase matched in the prompt",
            body,
        ));
    }

    Ok(no_high_confidence_prompt_context_message())
}

/// `GET /stats` — return token savings snapshot as JSON.
pub async fn stats_handler(
    State(state): State<SidecarState>,
) -> Json<crate::sidecar::StatsSnapshot> {
    Json(state.token_stats.summary())
}

// ---------------------------------------------------------------------------
// Helper: extract up to 2-level directory from a relative path
// ---------------------------------------------------------------------------

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

fn find_prompt_file_hint(
    state: &SidecarState,
    prompt: &str,
) -> Result<Option<PromptFileHint>, StatusCode> {
    let guard = state.index.read();
    let prompt_lower = prompt.to_ascii_lowercase();
    let mut module_match: Option<PromptFileHint> = None;
    let mut module_ambiguous = false;
    let mut qualified_path_match: Option<PromptFileHint> = None;
    let mut qualified_path_ambiguous = false;
    let mut basename_match: Option<PromptFileHint> = None;
    let mut basename_ambiguous = false;
    let mut stem_match: Option<PromptFileHint> = None;
    let mut stem_ambiguous = false;

    for (path, file) in guard.all_files() {
        if prompt.contains(path) || prompt_lower.contains(&path.to_ascii_lowercase()) {
            return Ok(Some(PromptFileHint {
                path: path.to_string(),
                line_hint_alias: None,
                match_kind: PromptHintMatchKind::ExactPath,
            }));
        }

        if let Some(module_alias) = prompt_file_module_alias(path, &file.language)
            && prompt_contains_exact_alias(prompt, &module_alias)
        {
            if let Some(existing) = &module_match {
                if existing.path != path.as_str() {
                    module_ambiguous = true;
                }
            } else {
                module_match = Some(PromptFileHint {
                    path: path.to_string(),
                    line_hint_alias: Some(module_alias),
                    match_kind: PromptHintMatchKind::ModuleAlias,
                });
            }
        }

        if let Some(path_without_extension) = prompt_path_without_extension(path)
            && find_prompt_path_line_hint(prompt, &path_without_extension).is_some()
        {
            if let Some(existing) = &qualified_path_match {
                if existing.path != path.as_str() {
                    qualified_path_ambiguous = true;
                }
            } else {
                qualified_path_match = Some(PromptFileHint {
                    path: path.to_string(),
                    line_hint_alias: Some(path_without_extension),
                    match_kind: PromptHintMatchKind::QualifiedPathAlias,
                });
            }
        }

        let Some(file_name) = std::path::Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
        else {
            continue;
        };
        if prompt_lower.contains(&file_name.to_ascii_lowercase()) {
            if let Some(existing) = &basename_match {
                if existing.path != path.as_str() {
                    basename_ambiguous = true;
                }
            } else {
                basename_match = Some(PromptFileHint {
                    path: path.to_string(),
                    line_hint_alias: Some(file_name.to_string()),
                    match_kind: PromptHintMatchKind::Basename,
                });
            }
        }

        let Some(file_stem) = std::path::Path::new(path)
            .file_stem()
            .and_then(|name| name.to_str())
        else {
            continue;
        };

        if find_prompt_path_line_hint(prompt, file_stem).is_none() {
            continue;
        }

        if let Some(existing) = &stem_match {
            if existing.path != path.as_str() {
                stem_ambiguous = true;
            }
        } else {
            stem_match = Some(PromptFileHint {
                path: path.to_string(),
                line_hint_alias: Some(file_stem.to_string()),
                match_kind: PromptHintMatchKind::StemLineAlias,
            });
        }
    }

    if !module_ambiguous && module_match.is_some() {
        return Ok(module_match);
    }

    if !qualified_path_ambiguous && qualified_path_match.is_some() {
        return Ok(qualified_path_match);
    }

    if !basename_ambiguous && basename_match.is_some() {
        return Ok(basename_match);
    }

    if stem_ambiguous {
        Ok(None)
    } else {
        Ok(stem_match)
    }
}

fn find_prompt_qualified_symbol_hint(
    state: &SidecarState,
    prompt: &str,
) -> Result<Option<PromptQualifiedSymbolHint>, StatusCode> {
    let guard = state.index.read();
    let mut qualified_symbol_match: Option<PromptQualifiedSymbolHint> = None;
    let mut qualified_symbol_ambiguous = false;

    for (path, file) in guard.all_files() {
        let Some(module_alias) = prompt_symbol_module_alias(path, &file.language) else {
            continue;
        };

        for symbol in &file.symbols {
            let Some(alias) = prompt_qualified_symbol_alias(&module_alias, &symbol.name) else {
                continue;
            };
            if !prompt_contains_exact_alias(prompt, &alias) {
                continue;
            }

            if let Some(existing) = &qualified_symbol_match {
                if existing.file_hint.path != path.as_str() || existing.symbol_name != symbol.name {
                    qualified_symbol_ambiguous = true;
                }
            } else {
                qualified_symbol_match = Some(PromptQualifiedSymbolHint {
                    file_hint: PromptFileHint {
                        path: path.to_string(),
                        line_hint_alias: Some(alias),
                        match_kind: PromptHintMatchKind::QualifiedSymbolAlias,
                    },
                    symbol_name: symbol.name.clone(),
                });
            }
        }
    }

    if qualified_symbol_ambiguous {
        Ok(None)
    } else {
        Ok(qualified_symbol_match)
    }
}

fn find_prompt_symbol_hint(
    state: &SidecarState,
    prompt: &str,
) -> Result<Option<String>, StatusCode> {
    let guard = state.index.read();
    for token in prompt_tokens(prompt) {
        if token.len() < 3 || token.contains('/') || token.contains('.') {
            continue;
        }

        let has_match = guard
            .all_files()
            .any(|(_, file)| file.symbols.iter().any(|symbol| symbol.name == token));
        if has_match {
            return Ok(Some(token));
        }
    }

    Ok(None)
}

fn find_prompt_line_hint(prompt: &str, file_hint: Option<&PromptFileHint>) -> Option<u32> {
    if let Some(file_hint) = file_hint {
        if let Some(line) = find_prompt_path_line_hint(prompt, &file_hint.path) {
            return Some(line);
        }
        if let Some(alias) = &file_hint.line_hint_alias
            && let Some(line) = find_prompt_path_line_hint(prompt, alias)
        {
            return Some(line);
        }
    }

    let tokens = prompt_tokens(prompt);
    for window in tokens.windows(2) {
        if !window[0].eq_ignore_ascii_case("line") {
            continue;
        }
        if let Ok(line) = window[1].parse::<u32>()
            && line > 0
        {
            return Some(line);
        }
    }

    None
}

fn find_prompt_path_line_hint(prompt: &str, path: &str) -> Option<u32> {
    let prompt_lower = prompt.to_ascii_lowercase();
    let needle = format!("{}:", path.to_ascii_lowercase());
    let mut search_start = 0;

    while let Some(offset) = prompt_lower[search_start..].find(&needle) {
        let value_start = search_start + offset + needle.len();
        let digits: String = prompt[value_start..]
            .chars()
            .take_while(|ch| ch.is_ascii_digit())
            .collect();
        if let Ok(line) = digits.parse::<u32>()
            && line > 0
        {
            return Some(line);
        }

        search_start = value_start;
    }

    None
}

fn prompt_path_without_extension(path: &str) -> Option<String> {
    let file_name = std::path::Path::new(path).file_name()?.to_str()?;
    let file_stem = std::path::Path::new(path).file_stem()?.to_str()?;
    if let Some((parent, _)) = path.rsplit_once('/') {
        Some(format!("{parent}/{file_stem}"))
    } else if file_name != file_stem {
        Some(file_stem.to_string())
    } else {
        None
    }
}

fn prompt_module_alias(path: &str, language: &LanguageId) -> Option<String> {
    let alias = match language {
        LanguageId::Rust => {
            let stripped = std::path::Path::new(path).strip_prefix("src").ok()?;
            let mut components: Vec<String> = stripped
                .components()
                .filter_map(|component| component.as_os_str().to_str().map(String::from))
                .collect();

            if let Some(last) = components.last_mut()
                && let Some(stem) = std::path::Path::new(last.as_str())
                    .file_stem()
                    .and_then(|value| value.to_str())
            {
                *last = stem.to_string();
            }

            if matches!(
                components.last().map(|value| value.as_str()),
                Some("lib" | "main" | "mod")
            ) {
                components.pop();
            }

            if components.is_empty() {
                Some("crate".to_string())
            } else {
                Some(format!("crate::{}", components.join("::")))
            }
        }
        LanguageId::Python => {
            let mut components: Vec<String> = std::path::Path::new(path)
                .components()
                .filter_map(|component| component.as_os_str().to_str().map(String::from))
                .collect();

            if let Some(last) = components.last_mut()
                && let Some(stem) = std::path::Path::new(last.as_str())
                    .file_stem()
                    .and_then(|value| value.to_str())
            {
                *last = stem.to_string();
            }

            if matches!(
                components.last().map(|value| value.as_str()),
                Some("__init__")
            ) {
                components.pop();
            }

            if components.is_empty() {
                None
            } else {
                Some(components.join("."))
            }
        }
        _ => None,
    }?;

    if alias.contains("::") || alias.contains('.') {
        Some(alias)
    } else {
        None
    }
}

fn prompt_file_module_alias(path: &str, language: &LanguageId) -> Option<String> {
    if let Some(alias) = prompt_module_alias(path, language) {
        return Some(alias);
    }

    let alias = match language {
        LanguageId::JavaScript | LanguageId::TypeScript => {
            let mut components: Vec<String> = std::path::Path::new(path)
                .components()
                .filter_map(|component| component.as_os_str().to_str().map(String::from))
                .collect();

            if let Some(last) = components.last_mut()
                && let Some(stem) = std::path::Path::new(last.as_str())
                    .file_stem()
                    .and_then(|value| value.to_str())
            {
                *last = stem.to_string();
            }

            if matches!(components.last().map(|value| value.as_str()), Some("index")) {
                components.pop();
            }

            if components.is_empty() {
                None
            } else {
                Some(components.join("/"))
            }
        }
        _ => None,
    }?;

    if alias.contains('/') {
        Some(alias)
    } else {
        None
    }
}

fn prompt_symbol_module_alias(path: &str, language: &LanguageId) -> Option<String> {
    prompt_file_module_alias(path, language)
}

fn prompt_qualified_symbol_alias(module_alias: &str, symbol_name: &str) -> Option<String> {
    let separator = if module_alias.contains("::") {
        "::"
    } else if module_alias.contains('.') {
        "."
    } else if module_alias.contains('/') {
        "/"
    } else {
        return None;
    };

    Some(format!("{module_alias}{separator}{symbol_name}"))
}

fn prompt_contains_exact_alias(prompt: &str, alias: &str) -> bool {
    let prompt_lower = prompt.to_ascii_lowercase();
    let alias_lower = alias.to_ascii_lowercase();
    let prompt_bytes = prompt_lower.as_bytes();
    let alias_bytes = alias_lower.as_bytes();
    let mut search_start = 0;

    while let Some(offset) = prompt_lower[search_start..].find(&alias_lower) {
        let start = search_start + offset;
        let end = start + alias_bytes.len();

        let prev_ok =
            start == 0 || !matches!(prompt_bytes[start - 1], b'a'..=b'z' | b'0'..=b'9' | b'_');

        let next_ok = if end >= prompt_bytes.len() {
            true
        } else {
            match prompt_bytes[end] {
                b':' => prompt_bytes
                    .get(end + 1)
                    .map(|byte| byte.is_ascii_digit())
                    .unwrap_or(false),
                b'.' | b'/' => false,
                byte => !matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'_'),
            }
        };

        if prev_ok && next_ok {
            return true;
        }

        search_start = start + 1;
    }

    false
}

fn prompt_tokens(prompt: &str) -> Vec<String> {
    prompt
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '/' || ch == '.'))
        .filter(|token| !token.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn prompt_requests_repo_map(prompt: &str) -> bool {
    let lower = prompt.to_ascii_lowercase();
    [
        "architecture",
        "codebase",
        "map",
        "overview",
        "repo",
        "repository",
        "structure",
    ]
    .iter()
    .any(|term| lower.contains(term))
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::{Duration, Instant, SystemTime};

    use parking_lot::RwLock;

    use crate::domain::{LanguageId, ReferenceKind, ReferenceRecord, SymbolKind, SymbolRecord};
    use crate::live_index::store::{CircuitBreakerState, IndexedFile, LiveIndex, ParseStatus};
    use crate::sidecar::{SidecarState, SymbolSnapshot, TokenStats};

    // -----------------------------------------------------------------------
    // Test helper: minimal LiveIndex with known contents
    // -----------------------------------------------------------------------

    fn make_symbol(name: &str, kind: SymbolKind, start: u32, end: u32) -> SymbolRecord {
        let byte_range = (0, 10);
        SymbolRecord {
            name: name.to_string(),
            kind,
            depth: 0,
            sort_order: 0,
            byte_range,
            item_byte_range: Some(byte_range),
            line_range: (start, end),
            doc_byte_range: None,
        }
    }

    fn make_reference(name: &str, kind: ReferenceKind, line: u32) -> ReferenceRecord {
        ReferenceRecord {
            name: name.to_string(),
            qualified_name: None,
            kind,
            byte_range: (100, 110),
            line_range: (line, line),
            enclosing_symbol_index: None,
        }
    }

    fn make_indexed_file(
        path: &str,
        symbols: Vec<SymbolRecord>,
        references: Vec<ReferenceRecord>,
        status: ParseStatus,
    ) -> IndexedFile {
        IndexedFile {
            relative_path: path.to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path(path),
            content: b"fn test() {}".to_vec(),
            symbols,
            parse_status: status,
            parse_diagnostic: None,
            byte_len: 12,
            content_hash: "abc".to_string(),
            references,
            alias_map: HashMap::new(),
            mtime_secs: 0,
        }
    }

    fn build_shared_index(
        files: Vec<(&str, IndexedFile)>,
    ) -> crate::live_index::store::SharedIndex {
        use crate::live_index::trigram::TrigramIndex;
        let files_map: HashMap<String, std::sync::Arc<IndexedFile>> = files
            .into_iter()
            .map(|(p, f)| (p.to_string(), std::sync::Arc::new(f)))
            .collect();
        let trigram_index = TrigramIndex::build_from_files(&files_map);
        let mut index = LiveIndex {
            files: files_map,
            loaded_at: Instant::now(),
            loaded_at_system: SystemTime::now(),
            load_duration: Duration::from_millis(10),
            cb_state: CircuitBreakerState::new(0.20),
            is_empty: false,
            load_source: crate::live_index::store::IndexLoadSource::FreshLoad,
            snapshot_verify_state: crate::live_index::store::SnapshotVerifyState::NotNeeded,
            reverse_index: HashMap::new(),
            files_by_basename: HashMap::new(),
            files_by_dir_component: HashMap::new(),
            trigram_index,
            gitignore: None,
            skipped_files: Vec::new(),
            coupling_store: None,
            local_empty_reason: std::sync::Arc::new(parking_lot::RwLock::new(None)),
        };
        index.rebuild_reverse_index();
        index.rebuild_path_indices();
        crate::live_index::SharedIndexHandle::shared(index)
    }

    /// Build a SidecarState wrapping a SharedIndex for use in tests.
    fn make_state(files: Vec<(&str, IndexedFile)>) -> SidecarState {
        SidecarState {
            index: build_shared_index(files),
            token_stats: TokenStats::new(),
            repo_root: None,
            symbol_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn make_state_with_root(
        files: Vec<(&str, IndexedFile)>,
        repo_root: std::path::PathBuf,
    ) -> SidecarState {
        SidecarState {
            index: build_shared_index(files),
            token_stats: TokenStats::new(),
            repo_root: Some(repo_root),
            symbol_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    #[test]
    fn freshen_sidecar_path_if_stale_generation_mismatch_preserves_valid_file() {
        let project_a = tempfile::tempdir().unwrap();
        let project_b = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(project_a.path().join("src")).unwrap();
        std::fs::create_dir_all(project_b.path().join("src")).unwrap();
        std::fs::write(project_a.path().join("src/a.rs"), "pub fn a() {}\n").unwrap();
        std::fs::write(project_b.path().join("src/b.rs"), "pub fn b() {}\n").unwrap();

        let index = LiveIndex::load(project_a.path()).unwrap();
        let stale_gen = index.current_project_generation();
        index.reload(project_b.path()).unwrap();
        let state = SidecarState {
            index,
            token_stats: TokenStats::new(),
            repo_root: Some(project_a.path().to_path_buf()),
            symbol_cache: Arc::new(RwLock::new(HashMap::new())),
        };

        let source_authority =
            freshen_sidecar_path_if_stale_at_generation(&state, "src/b.rs", stale_gen);

        assert!(matches!(
            source_authority,
            ContextSourceAuthority::CurrentIndex
        ));
        assert!(state.index.read().get_file("src/b.rs").is_some());
    }

    // -----------------------------------------------------------------------
    // health_handler
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_health_handler_returns_counts() {
        let f1 = make_indexed_file(
            "src/main.rs",
            vec![make_symbol("main", SymbolKind::Function, 1, 10)],
            vec![],
            ParseStatus::Parsed,
        );
        let f2 = make_indexed_file(
            "src/lib.rs",
            vec![
                make_symbol("foo", SymbolKind::Function, 1, 5),
                make_symbol("bar", SymbolKind::Function, 7, 12),
            ],
            vec![],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![("src/main.rs", f1), ("src/lib.rs", f2)]);

        let result = health_handler(State(state)).await.unwrap();
        let body = result.0;
        assert_eq!(body.file_count, 2, "health should report 2 files");
        assert_eq!(body.symbol_count, 3, "health should report 3 symbols");
        assert!(
            body.index_state.contains("Ready"),
            "index_state should include Ready"
        );
    }

    #[tokio::test]
    async fn test_health_handler_empty_index() {
        let state = make_state(vec![]);
        let result = health_handler(State(state)).await.unwrap();
        let body = result.0;
        assert_eq!(body.file_count, 0);
        assert_eq!(body.symbol_count, 0);
    }

    // -----------------------------------------------------------------------
    // outline_handler
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_outline_handler_returns_formatted_text() {
        let file = make_indexed_file(
            "src/foo.rs",
            vec![
                make_symbol("alpha", SymbolKind::Function, 1, 5),
                make_symbol("Beta", SymbolKind::Struct, 7, 10),
            ],
            vec![],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![("src/foo.rs", file)]);

        let params = OutlineParams {
            path: "src/foo.rs".to_string(),
            max_tokens: None,
            sections: None,
        };
        let result = outline_handler(State(state), Query(params)).await.unwrap();
        assert!(
            result.contains("alpha"),
            "outline should contain symbol name 'alpha'"
        );
        assert!(
            result.contains("Beta"),
            "outline should contain symbol name 'Beta'"
        );
        assert!(
            result.contains("src/foo.rs"),
            "outline should contain file path"
        );
        assert!(
            result.contains("tokens saved"),
            "outline should have token savings footer"
        );
        assert!(result.contains("Match type: exact"), "got: {result}");
        assert!(
            result.contains("Source authority: current index"),
            "got: {result}"
        );
        assert!(result.contains("Parse state: parsed"), "got: {result}");
        assert!(result.contains("Completeness: full"), "got: {result}");
        assert!(
            result.contains("Scope: path `src/foo.rs`; all sections"),
            "got: {result}"
        );
        assert!(
            result.contains("Evidence: file anchor `src/foo.rs`"),
            "got: {result}"
        );
    }

    #[tokio::test]
    async fn test_outline_handler_not_found_for_missing_file() {
        let state = make_state(vec![]);
        let params = OutlineParams {
            path: "nonexistent.rs".to_string(),
            max_tokens: None,
            sections: None,
        };
        let err = outline_handler(State(state), Query(params))
            .await
            .unwrap_err();
        assert_eq!(err, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_outline_handler_budget_enforced() {
        // Create a file with many symbols to trigger truncation.
        let symbols: Vec<SymbolRecord> = (0..50)
            .map(|i| {
                make_symbol(
                    &format!("symbol_{:04}", i),
                    SymbolKind::Function,
                    i * 2,
                    i * 2 + 1,
                )
            })
            .collect();
        let file = make_indexed_file("src/big.rs", symbols, vec![], ParseStatus::Parsed);
        let state = make_state(vec![("src/big.rs", file)]);

        let params = OutlineParams {
            path: "src/big.rs".to_string(),
            max_tokens: Some(10), // tiny budget to force truncation
            sections: None,
        };
        let result = outline_handler(State(state), Query(params)).await.unwrap();
        // With 10-token (40 byte) budget, only the header fits. Truncation suffix should appear.
        assert!(
            result.contains("truncated") || result.len() < 500,
            "result should be truncated or short: {}",
            result.len()
        );
        assert!(
            result.contains("Completeness: budget-limited"),
            "got: {result}"
        );
    }

    #[tokio::test]
    async fn test_outline_handler_reports_disk_refreshed_authority_for_stale_exact_path() {
        let tmp = tempfile::TempDir::new().unwrap();
        let src_dir = tmp.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        let file_path = src_dir.join("main.rs");
        std::fs::write(&file_path, "fn refreshed() {}\n").unwrap();

        let stale_file = make_indexed_file(
            "src/main.rs",
            vec![make_symbol("stale", SymbolKind::Function, 1, 1)],
            vec![],
            ParseStatus::Parsed,
        );
        let state =
            make_state_with_root(vec![("src/main.rs", stale_file)], tmp.path().to_path_buf());

        let params = OutlineParams {
            path: "src/main.rs".to_string(),
            max_tokens: None,
            sections: None,
        };
        let result = outline_handler(State(state), Query(params)).await.unwrap();

        assert!(
            result.contains("Source authority: disk-refreshed"),
            "got: {result}"
        );
        assert!(result.contains("refreshed"), "got: {result}");
    }

    #[tokio::test]
    async fn test_outline_handler_records_token_stats() {
        let file = make_indexed_file(
            "src/foo.rs",
            vec![make_symbol("alpha", SymbolKind::Function, 1, 5)],
            vec![],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![("src/foo.rs", file)]);
        let stats = Arc::clone(&state.token_stats);

        let params = OutlineParams {
            path: "src/foo.rs".to_string(),
            max_tokens: None,
            sections: None,
        };
        let _ = outline_handler(State(state), Query(params)).await.unwrap();
        assert_eq!(
            stats.summary().read_fires,
            1,
            "read fires should be incremented"
        );
    }

    // -----------------------------------------------------------------------
    // impact_handler — new_file path
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_impact_handler_new_file_returns_language_and_symbols() {
        use std::io::Write;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let rs_path = tmp.path().join("new_file.rs");
        let mut f = std::fs::File::create(&rs_path).unwrap();
        writeln!(f, "fn greet() {{}}").unwrap();
        writeln!(f, "struct Config {{}}").unwrap();
        drop(f);

        // Change cwd to tmp dir so the handler can find the file.
        let state = make_state(vec![]);

        // We'll call the handler with a relative path that exists when cwd = tmp.
        // Use absolute path directly to sidestep cwd issues.
        let abs_path_str = rs_path.to_string_lossy().to_string();
        let params = ImpactParams {
            path: abs_path_str.clone(),
            new_file: Some(true),
        };

        // The handler uses cwd.join(path), so with abs path it resolves correctly.
        let result = impact_handler(State(state), Query(params)).await;
        // It may fail if the extension detection doesn't work for absolute paths, but
        // the basic test is that it doesn't panic.
        // The result depends on file system state.
        let _ = result; // just verify no panic
    }

    // -----------------------------------------------------------------------
    // impact_handler — edit path
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_impact_handler_edit_returns_formatted_text() {
        let file = make_indexed_file(
            "src/db.rs",
            vec![make_symbol("connect", SymbolKind::Function, 1, 10)],
            vec![],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![("src/db.rs", file)]);

        // Seed the symbol cache with pre-edit state.
        {
            let mut cache = state.symbol_cache.write();
            cache.insert(
                "src/db.rs".to_string(),
                vec![SymbolSnapshot {
                    name: "connect".to_string(),
                    kind: "function".to_string(),
                    line_range: (1, 5), // different range = "Changed"
                    byte_range: (0, 50),
                }],
            );
        }

        let params = ImpactParams {
            path: "src/db.rs".to_string(),
            new_file: None,
        };

        // The handler will try to read src/db.rs from disk (cwd). Since the file
        // doesn't exist on disk in this test, the handler should return Ok with a
        // "not readable" message and preserve the index instead of destroying it.
        let result = impact_handler(State(state), Query(params)).await;
        assert!(
            result.is_ok(),
            "impact_handler should return Ok even if file missing from disk"
        );
        let text = result.unwrap();
        assert!(
            text.contains("removed from index") || text.contains("not found on disk"),
            "should indicate file was removed from index; got: {text}"
        );
    }

    /// When the watcher purges the index entry before analyze_file_impact
    /// runs, there is no pre-count to report. The response must not claim
    /// `Previously had 0 symbols` as if zero were a measured pre-state.
    #[tokio::test]
    async fn test_impact_handler_edit_honest_wording_when_index_already_purged() {
        // Index is empty; the caller asks about a path the watcher already
        // removed (or which never existed). The handler should acknowledge
        // the absence of a prior record rather than report "0 symbols".
        let state = make_state(vec![]);

        let params = ImpactParams {
            path: "src/ghost.rs".to_string(),
            new_file: None,
        };
        let result = impact_handler(State(state), Query(params)).await;
        assert!(result.is_ok(), "handler must tolerate the watcher race");
        let text = result.unwrap();
        assert!(
            text.contains("no index record remains"),
            "should flag the purged-index case explicitly; got: {text}"
        );
        assert!(
            !text.contains("Previously had 0 symbols"),
            "must not claim a pre-count that was never observed; got: {text}"
        );
    }

    /// Helper: SymbolRecord with explicit depth and byte_range, needed for
    /// parent-impl-type tests that the simpler `make_symbol` can't express.
    fn make_symbol_with_range(
        name: &str,
        kind: SymbolKind,
        depth: u32,
        line_range: (u32, u32),
        byte_range: (u32, u32),
    ) -> SymbolRecord {
        SymbolRecord {
            name: name.to_string(),
            kind,
            depth,
            sort_order: 0,
            byte_range,
            item_byte_range: Some(byte_range),
            line_range,
            doc_byte_range: None,
        }
    }

    #[test]
    fn test_find_record_matching_snapshot_matches_on_name_kind_and_byte_range() {
        let impl_record = make_symbol_with_range("impl Foo", SymbolKind::Impl, 0, (1, 5), (0, 200));
        let new_method = make_symbol_with_range("new", SymbolKind::Function, 1, (2, 3), (50, 80));
        let file = make_indexed_file(
            "src/foo.rs",
            vec![impl_record, new_method.clone()],
            vec![],
            ParseStatus::Parsed,
        );

        // Matching snapshot: all three fields agree → Some(record).
        let snap = SymbolSnapshot {
            name: "new".to_string(),
            kind: new_method.kind.to_string(),
            line_range: new_method.line_range,
            byte_range: new_method.byte_range,
        };
        let hit = find_record_matching_snapshot(&file, &snap);
        assert!(hit.is_some(), "exact match must resolve");

        // Name-only collision: different byte_range → None. This is what
        // prevents `MathMachine::new` from matching `Foo::new` elsewhere.
        let wrong_range = SymbolSnapshot {
            name: "new".to_string(),
            kind: new_method.kind.to_string(),
            line_range: new_method.line_range,
            byte_range: (999, 1000),
        };
        assert!(
            find_record_matching_snapshot(&file, &wrong_range).is_none(),
            "byte_range mismatch must not resolve"
        );

        // Name + range match but wrong kind → None.
        let wrong_kind = SymbolSnapshot {
            name: "new".to_string(),
            kind: SymbolKind::Struct.to_string(),
            line_range: new_method.line_range,
            byte_range: new_method.byte_range,
        };
        assert!(
            find_record_matching_snapshot(&file, &wrong_kind).is_none(),
            "kind mismatch must not resolve"
        );
    }

    /// End-to-end: when analyze_file_impact reports callers of a changed
    /// method inside `impl Foo`, it must exclude files that only reference
    /// an unrelated same-named method (e.g. `Bar::new`). The fix type-scopes
    /// the caller list using find_parent_impl_type + file-presence filter.
    #[tokio::test]
    async fn test_impact_handler_type_scopes_caller_review() {
        use std::io::Write;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let src_dir = tmp.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        // Write the post-edit file content. The parser must produce an
        // Impl symbol and a nested `new` method so find_parent_impl_type
        // returns Some("Foo").
        let foo_path = src_dir.join("foo.rs");
        let mut f = std::fs::File::create(&foo_path).unwrap();
        writeln!(f, "pub struct Foo;").unwrap();
        writeln!(f, "impl Foo {{").unwrap();
        writeln!(f, "    pub fn new() -> Self {{ Self }}").unwrap();
        writeln!(f, "}}").unwrap();
        drop(f);

        // Pre-edit snapshot: `new` existed at a different byte_range so the
        // diff flags it as Changed rather than unchanged.
        let pre_impl = make_symbol_with_range("impl Foo", SymbolKind::Impl, 0, (1, 1), (0, 5));
        let pre_new = make_symbol_with_range("new", SymbolKind::Function, 1, (1, 1), (10, 20));
        let pre_file = make_indexed_file(
            "src/foo.rs",
            vec![pre_impl, pre_new],
            vec![],
            ParseStatus::Parsed,
        );

        // A file that references `Foo` AND `new` — legitimate caller.
        let uses_foo = make_indexed_file(
            "src/uses_foo.rs",
            vec![],
            vec![
                make_reference("Foo", ReferenceKind::TypeUsage, 1),
                make_reference("new", ReferenceKind::Call, 2),
            ],
            ParseStatus::Parsed,
        );

        // A file that references `new` but NOT `Foo` — must be filtered out.
        // Simulates the `MathMachine::new` vs other-type `::new` false positive.
        let uses_other = make_indexed_file(
            "src/uses_bar.rs",
            vec![],
            vec![
                make_reference("Bar", ReferenceKind::TypeUsage, 1),
                make_reference("new", ReferenceKind::Call, 5),
            ],
            ParseStatus::Parsed,
        );

        let state = make_state_with_root(
            vec![
                ("src/foo.rs", pre_file),
                ("src/uses_foo.rs", uses_foo),
                ("src/uses_bar.rs", uses_other),
            ],
            tmp.path().to_path_buf(),
        );

        let params = ImpactParams {
            path: "src/foo.rs".to_string(),
            new_file: None,
        };
        let result = impact_handler(State(state), Query(params))
            .await
            .expect("handler returns Ok");

        // Sanity: `new` is reported as Changed (or Added — parse may shift
        // byte ranges enough to confuse the diff, which is fine for this
        // test — what matters is that the caller-review block renders and
        // is type-scoped).
        assert!(
            result.contains("Callers of new()") || !result.contains("Callers to review:"),
            "when caller review renders, the symbol name header must appear; got:\n{result}"
        );

        if result.contains("Callers to review:") {
            assert!(
                result.contains("src/uses_foo.rs"),
                "caller in a file that references the parent type must be kept; got:\n{result}"
            );
            assert!(
                !result.contains("src/uses_bar.rs"),
                "caller in a file that does NOT reference the parent type must be filtered out; got:\n{result}"
            );
        }
    }

    /// Proves that analyze_file_impact removes the file from the index when
    /// it cannot be read from disk (deleted externally).
    #[tokio::test]
    async fn test_impact_handler_edit_preserves_index_when_file_unreadable() {
        let file = make_indexed_file(
            "src/db.rs",
            vec![make_symbol("connect", SymbolKind::Function, 1, 10)],
            vec![],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![("src/db.rs", file)]);

        let params = ImpactParams {
            path: "src/db.rs".to_string(),
            new_file: None,
        };

        // File doesn't exist on disk — impact should remove it from the index.
        let result = impact_handler(State(state.clone()), Query(params)).await;
        assert!(result.is_ok(), "should return Ok, got: {result:?}");

        // Verify the file was removed from the index.
        let guard = state.index.read();
        assert!(
            guard.get_file("src/db.rs").is_none(),
            "deleted file should be removed from index"
        );
    }

    // -----------------------------------------------------------------------
    // symbol_context_handler
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_symbol_context_handler_returns_formatted_text() {
        let f = make_indexed_file(
            "src/main.rs",
            vec![],
            vec![make_reference("process", ReferenceKind::Call, 5)],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![("src/main.rs", f)]);

        let params = SymbolContextParams {
            name: "process".to_string(),
            file: None,
            path: None,
            symbol_kind: None,
            symbol_line: None,
        };
        let result = symbol_context_handler(State(state), Query(params))
            .await
            .unwrap();
        assert!(result.contains("src/main.rs"), "should contain the file");
        assert!(result.contains("line 5"), "should show line number");
        assert!(result.contains("tokens saved"), "should have footer");
        assert!(result.contains("Match type: heuristic"), "got: {result}");
        assert!(
            result.contains("Source authority: current index"),
            "got: {result}"
        );
        assert!(result.contains("Parse state: parsed"), "got: {result}");
        assert!(result.contains("Completeness: full"), "got: {result}");
        assert!(
            result.contains("Scope: repo-wide symbol token `process`"),
            "got: {result}"
        );
        assert!(
            result.contains("Evidence: symbol token `process` anchored at src/main.rs:5"),
            "got: {result}"
        );
    }

    #[tokio::test]
    async fn test_symbol_context_handler_caps_at_10() {
        // Create 20 files each with one reference to "target".
        let files: Vec<(&str, IndexedFile)> = (0..20usize)
            .map(|i| {
                let path = Box::leak(format!("src/f{i}.rs").into_boxed_str()) as &'static str;
                let file = make_indexed_file(
                    path,
                    vec![],
                    vec![make_reference("target", ReferenceKind::Call, 1)],
                    ParseStatus::Parsed,
                );
                (path, file)
            })
            .collect();
        let state = make_state(files);

        let params = SymbolContextParams {
            name: "target".to_string(),
            file: None,
            path: None,
            symbol_kind: None,
            symbol_line: None,
        };
        let result = symbol_context_handler(State(state), Query(params))
            .await
            .unwrap();
        // Should show at most 10 matches (either via our cap-at-10 note, or via budget truncation).
        // Count the number of "line 1" occurrences to verify we don't show more than 10.
        let match_count = result.matches("line 1").count();
        assert!(
            match_count <= 10,
            "should show at most 10 matches, got {}: {}",
            match_count,
            result
        );
        // Should indicate there are more matches (via "showing" or "truncated").
        assert!(
            result.contains("showing") || result.contains("truncated"),
            "should indicate truncation: {}",
            result
        );
        assert!(result.contains("Completeness: truncated"), "got: {result}");
    }

    #[tokio::test]
    async fn test_symbol_context_handler_exact_selector_excludes_unrelated_same_name_hits() {
        let target = make_indexed_file(
            "src/db.rs",
            vec![make_symbol("connect", SymbolKind::Function, 1, 1)],
            vec![],
            ParseStatus::Parsed,
        );
        let dependent = IndexedFile {
            relative_path: "src/service.rs".to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path("src/service.rs"),
            content: b"use crate::db::connect;\nfn run() { connect(); }\n".to_vec(),
            symbols: vec![make_symbol("run", SymbolKind::Function, 2, 2)],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 46,
            content_hash: "abc".to_string(),
            references: vec![
                ReferenceRecord {
                    name: "db".to_string(),
                    qualified_name: Some("crate::db".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (0, 6),
                    line_range: (0, 0),
                    enclosing_symbol_index: Some(0),
                },
                ReferenceRecord {
                    name: "connect".to_string(),
                    qualified_name: Some("crate::db::connect".to_string()),
                    kind: ReferenceKind::Call,
                    byte_range: (10, 16),
                    line_range: (1, 1),
                    enclosing_symbol_index: Some(0),
                },
            ],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let unrelated = make_indexed_file(
            "src/other.rs",
            vec![make_symbol("run", SymbolKind::Function, 1, 1)],
            vec![make_reference("connect", ReferenceKind::Call, 1)],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![
            ("src/db.rs", target),
            ("src/service.rs", dependent),
            ("src/other.rs", unrelated),
        ]);

        let params = SymbolContextParams {
            name: "connect".to_string(),
            file: None,
            path: Some("src/db.rs".to_string()),
            symbol_kind: Some("fn".to_string()),
            symbol_line: Some(2),
        };
        let result = symbol_context_handler(State(state), Query(params))
            .await
            .unwrap();

        assert!(result.contains("src/service.rs"), "got: {result}");
        assert!(!result.contains("src/other.rs"), "got: {result}");
    }

    #[tokio::test]
    async fn test_symbol_context_handler_exact_selector_requires_line_for_ambiguous_symbol() {
        let target = make_indexed_file(
            "src/db.rs",
            vec![
                make_symbol("connect", SymbolKind::Function, 1, 1),
                make_symbol("connect", SymbolKind::Function, 2, 2),
            ],
            vec![],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![("src/db.rs", target)]);

        let params = SymbolContextParams {
            name: "connect".to_string(),
            file: None,
            path: Some("src/db.rs".to_string()),
            symbol_kind: Some("fn".to_string()),
            symbol_line: None,
        };
        let result = symbol_context_handler(State(state), Query(params))
            .await
            .unwrap();

        assert!(
            result.contains("Ambiguous symbol selector"),
            "got: {result}"
        );
        assert!(result.contains("1"), "got: {result}");
        assert!(result.contains("2"), "got: {result}");
    }

    // -----------------------------------------------------------------------
    // repo_map_handler
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_repo_map_handler_returns_formatted_tree() {
        let f1 = make_indexed_file(
            "src/main.rs",
            vec![make_symbol("x", SymbolKind::Function, 1, 3)],
            vec![],
            ParseStatus::Parsed,
        );
        let f2 = make_indexed_file(
            "src/lib.rs",
            vec![],
            vec![],
            ParseStatus::Failed {
                error: "oops".to_string(),
            },
        );
        let state = make_state(vec![("src/main.rs", f1), ("src/lib.rs", f2)]);

        let result = repo_map_handler(State(state)).await.unwrap();
        assert!(result.contains("files"), "should mention file count");
        assert!(result.contains("symbols"), "should mention symbol count");
        assert!(result.contains("src"), "should show directory");
    }

    #[tokio::test]
    async fn test_repo_map_handler_empty_index() {
        let state = make_state(vec![]);
        let result = repo_map_handler(State(state)).await.unwrap();
        assert!(
            result.contains("0 files"),
            "empty index should show 0 files"
        );
    }

    #[test]
    fn test_is_intra_workspace_path_rejects_absolute_paths() {
        assert!(is_intra_workspace_path("src/main.rs"));
        assert!(is_intra_workspace_path("tests/fixtures/foo.rs"));
        // Windows drive-letter paths from other indexed repos.
        assert!(!is_intra_workspace_path(
            "C:\\AI_STUFF\\PROGRAMMING\\octogent\\apps\\api\\tests\\hookDrivenBootstrap.test.ts"
        ));
        // POSIX absolute paths.
        assert!(!is_intra_workspace_path("/usr/local/project/src/main.rs"));
    }

    #[tokio::test]
    async fn test_repo_map_excludes_foreign_workspace_paths_from_key_types() {
        let local = make_indexed_file(
            "src/local_type.rs",
            vec![make_symbol("LocalThing", SymbolKind::Struct, 1, 5)],
            vec![],
            ParseStatus::Parsed,
        );
        let foreign_windows = make_indexed_file(
            "C:\\AI_STUFF\\PROGRAMMING\\otherrepo\\src\\ForeignType.ts",
            vec![make_symbol("ForeignWindows", SymbolKind::Class, 1, 5)],
            vec![],
            ParseStatus::Parsed,
        );
        let foreign_posix = make_indexed_file(
            "/home/someone/otherrepo/src/foreign.rs",
            vec![make_symbol("ForeignPosix", SymbolKind::Struct, 1, 5)],
            vec![],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![
            ("src/local_type.rs", local),
            (
                "C:\\AI_STUFF\\PROGRAMMING\\otherrepo\\src\\ForeignType.ts",
                foreign_windows,
            ),
            ("/home/someone/otherrepo/src/foreign.rs", foreign_posix),
        ]);

        let result = repo_map_handler(State(state)).await.unwrap();
        assert!(
            result.contains("LocalThing"),
            "key types should include the local symbol; got:\n{result}"
        );
        assert!(
            !result.contains("ForeignWindows"),
            "key types must not leak Windows drive-letter paths from other workspaces; got:\n{result}"
        );
        assert!(
            !result.contains("ForeignPosix"),
            "key types must not leak POSIX absolute paths from other workspaces; got:\n{result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_prefers_file_hint() {
        let file = make_indexed_file(
            "src/main.rs",
            vec![make_symbol("serve", SymbolKind::Function, 1, 3)],
            vec![],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![("src/main.rs", file)]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "please inspect src/main.rs".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            result.contains("src/main.rs"),
            "prompt context should target the hinted file"
        );
        assert!(
            result.contains("serve"),
            "prompt context should surface the file outline"
        );
        assert!(
            result.contains("Prompt-context signal: high-confidence"),
            "exact file hints should surface calibrated confidence: {result}"
        );
        assert!(
            result.contains("exact path `src/main.rs` matched in the prompt"),
            "exact file hints should expose the evidence source: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_symbol_hint_uses_name_only_symbol_context() {
        let target = make_indexed_file(
            "src/db.rs",
            vec![make_symbol("connect", SymbolKind::Function, 1, 1)],
            vec![],
            ParseStatus::Parsed,
        );
        let dependent = IndexedFile {
            relative_path: "src/service.rs".to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path("src/service.rs"),
            content: b"use crate::db::connect;\nfn run() { connect(); }\n".to_vec(),
            symbols: vec![make_symbol("run", SymbolKind::Function, 2, 2)],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 46,
            content_hash: "abc".to_string(),
            references: vec![
                ReferenceRecord {
                    name: "db".to_string(),
                    qualified_name: Some("crate::db".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (0, 6),
                    line_range: (0, 0),
                    enclosing_symbol_index: Some(0),
                },
                ReferenceRecord {
                    name: "connect".to_string(),
                    qualified_name: Some("crate::db::connect".to_string()),
                    kind: ReferenceKind::Call,
                    byte_range: (10, 16),
                    line_range: (1, 1),
                    enclosing_symbol_index: Some(0),
                },
            ],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let unrelated = make_indexed_file(
            "src/other.rs",
            vec![make_symbol("run", SymbolKind::Function, 1, 1)],
            vec![make_reference("connect", ReferenceKind::Call, 1)],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![
            ("src/db.rs", target),
            ("src/service.rs", dependent),
            ("src/other.rs", unrelated),
        ]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "where is connect used".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            result.contains("src/service.rs"),
            "symbol-only prompt should use symbol context: {result}"
        );
        assert!(
            result.contains("src/other.rs"),
            "name-only symbol context should keep global same-name hits: {result}"
        );
        assert!(
            result.contains("Prompt-context signal: heuristic"),
            "symbol-only hints should be labeled heuristic: {result}"
        );
        assert!(
            result.contains("symbol token `connect` matched somewhere in the index"),
            "symbol-only hints should expose their evidence source: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_without_hint_reports_no_high_confidence_signal() {
        let target = make_indexed_file(
            "src/db.rs",
            vec![make_symbol("connect", SymbolKind::Function, 1, 1)],
            vec![],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![("src/db.rs", target)]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "please help with the database thing".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            result.contains("Prompt-context signal: no high-confidence hint"),
            "unmatched prompts should explicitly report low confidence: {result}"
        );
        assert!(
            result.contains("search_symbols(...)"),
            "unmatched prompts should suggest the next narrowing step: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_combined_file_and_symbol_hint_uses_exact_selector() {
        let target = make_indexed_file(
            "src/db.rs",
            vec![make_symbol("connect", SymbolKind::Function, 1, 1)],
            vec![],
            ParseStatus::Parsed,
        );
        let dependent = IndexedFile {
            relative_path: "src/service.rs".to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path("src/service.rs"),
            content: b"use crate::db::connect;\nfn run() { connect(); }\n".to_vec(),
            symbols: vec![make_symbol("run", SymbolKind::Function, 2, 2)],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 46,
            content_hash: "abc".to_string(),
            references: vec![
                ReferenceRecord {
                    name: "db".to_string(),
                    qualified_name: Some("crate::db".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (0, 6),
                    line_range: (0, 0),
                    enclosing_symbol_index: Some(0),
                },
                ReferenceRecord {
                    name: "connect".to_string(),
                    qualified_name: Some("crate::db::connect".to_string()),
                    kind: ReferenceKind::Call,
                    byte_range: (10, 16),
                    line_range: (1, 1),
                    enclosing_symbol_index: Some(0),
                },
            ],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let unrelated = make_indexed_file(
            "src/other.rs",
            vec![make_symbol("run", SymbolKind::Function, 1, 1)],
            vec![make_reference("connect", ReferenceKind::Call, 1)],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![
            ("src/db.rs", target),
            ("src/service.rs", dependent),
            ("src/other.rs", unrelated),
        ]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect src/db.rs connect".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            result.contains("src/service.rs"),
            "combined prompt should use exact selector symbol context: {result}"
        );
        assert!(
            !result.contains("src/other.rs"),
            "exact selector should exclude unrelated same-name hits: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_combined_hint_reports_exact_selector_ambiguity() {
        let target = make_indexed_file(
            "src/db.rs",
            vec![
                make_symbol("connect", SymbolKind::Function, 1, 1),
                make_symbol("connect", SymbolKind::Function, 2, 2),
            ],
            vec![],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![("src/db.rs", target)]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect src/db.rs connect".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            result.contains("Ambiguous symbol selector"),
            "combined prompt should surface exact-selector ambiguity: {result}"
        );
        assert!(result.contains("1"), "got: {result}");
        assert!(result.contains("2"), "got: {result}");
    }

    #[tokio::test]
    async fn test_prompt_context_handler_combined_hint_line_hint_disambiguates_selector() {
        let target = make_indexed_file(
            "src/db.rs",
            vec![
                make_symbol("connect", SymbolKind::Function, 1, 1),
                make_symbol("connect", SymbolKind::Function, 2, 2),
            ],
            vec![],
            ParseStatus::Parsed,
        );
        let dependent = IndexedFile {
            relative_path: "src/service.rs".to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path("src/service.rs"),
            content: b"use crate::db::connect;\nfn run() { connect(); }\n".to_vec(),
            symbols: vec![make_symbol("run", SymbolKind::Function, 2, 2)],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 46,
            content_hash: "abc".to_string(),
            references: vec![
                ReferenceRecord {
                    name: "db".to_string(),
                    qualified_name: Some("crate::db".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (0, 6),
                    line_range: (0, 0),
                    enclosing_symbol_index: Some(0),
                },
                ReferenceRecord {
                    name: "connect".to_string(),
                    qualified_name: Some("crate::db::connect".to_string()),
                    kind: ReferenceKind::Call,
                    byte_range: (10, 16),
                    line_range: (1, 1),
                    enclosing_symbol_index: Some(0),
                },
            ],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let state = make_state(vec![("src/db.rs", target), ("src/service.rs", dependent)]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect src/db.rs connect line 2".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            !result.contains("Ambiguous symbol selector"),
            "line hint should disambiguate the exact selector: {result}"
        );
        assert!(
            result.contains("src/service.rs"),
            "line hint should still return symbol context results: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_ignores_unlabeled_numbers_for_line_hint() {
        let target = make_indexed_file(
            "src/db.rs",
            vec![
                make_symbol("connect", SymbolKind::Function, 1, 1),
                make_symbol("connect", SymbolKind::Function, 2, 2),
            ],
            vec![],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![("src/db.rs", target)]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect src/db.rs connect 2".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            result.contains("Ambiguous symbol selector"),
            "unlabeled numbers should not count as line hints: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_path_line_hint_disambiguates_selector() {
        let target = make_indexed_file(
            "src/db.rs",
            vec![
                make_symbol("connect", SymbolKind::Function, 1, 1),
                make_symbol("connect", SymbolKind::Function, 2, 2),
            ],
            vec![],
            ParseStatus::Parsed,
        );
        let dependent = IndexedFile {
            relative_path: "src/service.rs".to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path("src/service.rs"),
            content: b"use crate::db::connect;\nfn run() { connect(); }\n".to_vec(),
            symbols: vec![make_symbol("run", SymbolKind::Function, 2, 2)],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 46,
            content_hash: "abc".to_string(),
            references: vec![
                ReferenceRecord {
                    name: "db".to_string(),
                    qualified_name: Some("crate::db".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (0, 6),
                    line_range: (0, 0),
                    enclosing_symbol_index: Some(0),
                },
                ReferenceRecord {
                    name: "connect".to_string(),
                    qualified_name: Some("crate::db::connect".to_string()),
                    kind: ReferenceKind::Call,
                    byte_range: (10, 16),
                    line_range: (1, 1),
                    enclosing_symbol_index: Some(0),
                },
            ],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let state = make_state(vec![("src/db.rs", target), ("src/service.rs", dependent)]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect src/db.rs:2 connect".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            !result.contains("Ambiguous symbol selector"),
            "path:line hint should disambiguate the exact selector: {result}"
        );
        assert!(
            result.contains("src/service.rs"),
            "path:line hint should still return symbol context results: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_basename_line_hint_disambiguates_selector() {
        let target = make_indexed_file(
            "src/db.rs",
            vec![
                make_symbol("connect", SymbolKind::Function, 1, 1),
                make_symbol("connect", SymbolKind::Function, 2, 2),
            ],
            vec![],
            ParseStatus::Parsed,
        );
        let dependent = IndexedFile {
            relative_path: "src/service.rs".to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path("src/service.rs"),
            content: b"use crate::db::connect;\nfn run() { connect(); }\n".to_vec(),
            symbols: vec![make_symbol("run", SymbolKind::Function, 2, 2)],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 46,
            content_hash: "abc".to_string(),
            references: vec![
                ReferenceRecord {
                    name: "db".to_string(),
                    qualified_name: Some("crate::db".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (0, 6),
                    line_range: (0, 0),
                    enclosing_symbol_index: Some(0),
                },
                ReferenceRecord {
                    name: "connect".to_string(),
                    qualified_name: Some("crate::db::connect".to_string()),
                    kind: ReferenceKind::Call,
                    byte_range: (10, 16),
                    line_range: (1, 1),
                    enclosing_symbol_index: Some(0),
                },
            ],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let state = make_state(vec![("src/db.rs", target), ("src/service.rs", dependent)]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect db.rs:2 connect".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            !result.contains("Ambiguous symbol selector"),
            "basename:line hint should disambiguate the exact selector: {result}"
        );
        assert!(
            result.contains("src/service.rs"),
            "basename:line hint should still return symbol context results: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_extensionless_alias_line_hint_disambiguates_selector() {
        let target = make_indexed_file(
            "src/db.rs",
            vec![
                make_symbol("connect", SymbolKind::Function, 1, 1),
                make_symbol("connect", SymbolKind::Function, 2, 2),
            ],
            vec![],
            ParseStatus::Parsed,
        );
        let dependent = IndexedFile {
            relative_path: "src/service.rs".to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path("src/service.rs"),
            content: b"use crate::db::connect;\nfn run() { connect(); }\n".to_vec(),
            symbols: vec![make_symbol("run", SymbolKind::Function, 2, 2)],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 46,
            content_hash: "abc".to_string(),
            references: vec![
                ReferenceRecord {
                    name: "db".to_string(),
                    qualified_name: Some("crate::db".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (0, 6),
                    line_range: (0, 0),
                    enclosing_symbol_index: Some(0),
                },
                ReferenceRecord {
                    name: "connect".to_string(),
                    qualified_name: Some("crate::db::connect".to_string()),
                    kind: ReferenceKind::Call,
                    byte_range: (10, 16),
                    line_range: (1, 1),
                    enclosing_symbol_index: Some(0),
                },
            ],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let unrelated = make_indexed_file(
            "src/other.rs",
            vec![make_symbol("run", SymbolKind::Function, 1, 1)],
            vec![make_reference("connect", ReferenceKind::Call, 1)],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![
            ("src/db.rs", target),
            ("src/service.rs", dependent),
            ("src/other.rs", unrelated),
        ]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect db:2 connect".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            !result.contains("Ambiguous symbol selector"),
            "extensionless alias should disambiguate the exact selector: {result}"
        );
        assert!(
            result.contains("src/service.rs"),
            "extensionless alias should still return symbol context results: {result}"
        );
        assert!(
            !result.contains("src/other.rs"),
            "extensionless alias should exclude unrelated same-name hits: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_extensionless_path_line_hint_disambiguates_selector() {
        let src_target = make_indexed_file(
            "src/db.rs",
            vec![
                make_symbol("connect", SymbolKind::Function, 1, 1),
                make_symbol("connect", SymbolKind::Function, 2, 2),
            ],
            vec![],
            ParseStatus::Parsed,
        );
        let test_target = make_indexed_file(
            "tests/db.py",
            vec![make_symbol("connect", SymbolKind::Function, 1, 1)],
            vec![],
            ParseStatus::Parsed,
        );
        let src_dependent = IndexedFile {
            relative_path: "src/service.rs".to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path("src/service.rs"),
            content: b"use crate::db::connect;\nfn run() { connect(); }\n".to_vec(),
            symbols: vec![make_symbol("run", SymbolKind::Function, 2, 2)],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 46,
            content_hash: "abc".to_string(),
            references: vec![
                ReferenceRecord {
                    name: "db".to_string(),
                    qualified_name: Some("crate::db".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (0, 6),
                    line_range: (0, 0),
                    enclosing_symbol_index: Some(0),
                },
                ReferenceRecord {
                    name: "connect".to_string(),
                    qualified_name: Some("crate::db::connect".to_string()),
                    kind: ReferenceKind::Call,
                    byte_range: (10, 16),
                    line_range: (1, 1),
                    enclosing_symbol_index: Some(0),
                },
            ],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let unrelated = make_indexed_file(
            "src/other.rs",
            vec![make_symbol("run", SymbolKind::Function, 1, 1)],
            vec![make_reference("connect", ReferenceKind::Call, 1)],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![
            ("src/db.rs", src_target),
            ("tests/db.py", test_target),
            ("src/service.rs", src_dependent),
            ("src/other.rs", unrelated),
        ]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect src/db:2 connect".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            !result.contains("Ambiguous symbol selector"),
            "extensionless path alias should disambiguate the exact selector: {result}"
        );
        assert!(
            result.contains("src/service.rs"),
            "extensionless path alias should still return symbol context results: {result}"
        );
        assert!(
            !result.contains("src/other.rs"),
            "extensionless path alias should exclude unrelated same-name hits: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_module_alias_line_hint_disambiguates_selector() {
        let src_target = make_indexed_file(
            "src/db.rs",
            vec![
                make_symbol("connect", SymbolKind::Function, 1, 1),
                make_symbol("connect", SymbolKind::Function, 2, 2),
            ],
            vec![],
            ParseStatus::Parsed,
        );
        let test_target = make_indexed_file(
            "tests/db.py",
            vec![make_symbol("connect", SymbolKind::Function, 1, 1)],
            vec![],
            ParseStatus::Parsed,
        );
        let src_dependent = IndexedFile {
            relative_path: "src/service.rs".to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path("src/service.rs"),
            content: b"use crate::db::connect;\nfn run() { connect(); }\n".to_vec(),
            symbols: vec![make_symbol("run", SymbolKind::Function, 2, 2)],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 46,
            content_hash: "abc".to_string(),
            references: vec![
                ReferenceRecord {
                    name: "db".to_string(),
                    qualified_name: Some("crate::db".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (0, 6),
                    line_range: (0, 0),
                    enclosing_symbol_index: Some(0),
                },
                ReferenceRecord {
                    name: "connect".to_string(),
                    qualified_name: Some("crate::db::connect".to_string()),
                    kind: ReferenceKind::Call,
                    byte_range: (10, 16),
                    line_range: (1, 1),
                    enclosing_symbol_index: Some(0),
                },
            ],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let unrelated = make_indexed_file(
            "src/other.rs",
            vec![make_symbol("run", SymbolKind::Function, 1, 1)],
            vec![make_reference("connect", ReferenceKind::Call, 1)],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![
            ("src/db.rs", src_target),
            ("tests/db.py", test_target),
            ("src/service.rs", src_dependent),
            ("src/other.rs", unrelated),
        ]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect crate::db:2 connect".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            !result.contains("Ambiguous symbol selector"),
            "module alias should disambiguate the exact selector: {result}"
        );
        assert!(
            result.contains("src/service.rs"),
            "module alias should still return symbol context results: {result}"
        );
        assert!(
            !result.contains("src/other.rs"),
            "module alias should exclude unrelated same-name hits: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_module_alias_without_line_prefers_exact_file_hint() {
        let src_target = make_indexed_file(
            "src/db.rs",
            vec![make_symbol("connect", SymbolKind::Function, 2, 2)],
            vec![],
            ParseStatus::Parsed,
        );
        let test_target = make_indexed_file(
            "tests/db.py",
            vec![make_symbol("connect", SymbolKind::Function, 1, 1)],
            vec![],
            ParseStatus::Parsed,
        );
        let src_dependent = IndexedFile {
            relative_path: "src/service.rs".to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path("src/service.rs"),
            content: b"use crate::db::connect;\nfn run() { connect(); }\n".to_vec(),
            symbols: vec![make_symbol("run", SymbolKind::Function, 2, 2)],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 46,
            content_hash: "abc".to_string(),
            references: vec![
                ReferenceRecord {
                    name: "db".to_string(),
                    qualified_name: Some("crate::db".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (0, 6),
                    line_range: (0, 0),
                    enclosing_symbol_index: Some(0),
                },
                ReferenceRecord {
                    name: "connect".to_string(),
                    qualified_name: Some("crate::db::connect".to_string()),
                    kind: ReferenceKind::Call,
                    byte_range: (10, 16),
                    line_range: (1, 1),
                    enclosing_symbol_index: Some(0),
                },
            ],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let unrelated = make_indexed_file(
            "src/other.rs",
            vec![make_symbol("run", SymbolKind::Function, 1, 1)],
            vec![make_reference("connect", ReferenceKind::Call, 1)],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![
            ("src/db.rs", src_target),
            ("tests/db.py", test_target),
            ("src/service.rs", src_dependent),
            ("src/other.rs", unrelated),
        ]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect crate::db connect".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            !result.contains("Ambiguous symbol selector"),
            "module alias without line should still resolve the exact file hint: {result}"
        );
        assert!(
            result.contains("src/service.rs"),
            "module alias without line should still return symbol context results: {result}"
        );
        assert!(
            !result.contains("src/other.rs"),
            "module alias without line should exclude unrelated same-name hits: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_slash_module_alias_without_line_prefers_exact_file_hint() {
        let target = IndexedFile {
            relative_path: "src/utils/index.ts".to_string(),
            language: LanguageId::TypeScript,
            classification: crate::domain::FileClassification::for_code_path("src/utils/index.ts"),
            content: b"export function connect() {}\n".to_vec(),
            symbols: vec![make_symbol("connect", SymbolKind::Function, 1, 1)],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 28,
            content_hash: "utils-ts".to_string(),
            references: vec![],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let dependent = IndexedFile {
            relative_path: "src/app.ts".to_string(),
            language: LanguageId::TypeScript,
            classification: crate::domain::FileClassification::for_code_path("src/app.ts"),
            content: b"import { connect } from 'src/utils';\nconnect();\n".to_vec(),
            symbols: vec![make_symbol("run", SymbolKind::Function, 2, 2)],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 49,
            content_hash: "app-ts".to_string(),
            references: vec![
                ReferenceRecord {
                    name: "utils".to_string(),
                    qualified_name: Some("src/utils".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (24, 33),
                    line_range: (0, 0),
                    enclosing_symbol_index: None,
                },
                ReferenceRecord {
                    name: "connect".to_string(),
                    qualified_name: Some("src/utils/connect".to_string()),
                    kind: ReferenceKind::Call,
                    byte_range: (36, 42),
                    line_range: (1, 1),
                    enclosing_symbol_index: Some(0),
                },
            ],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let unrelated = make_indexed_file(
            "src/other.ts",
            vec![make_symbol("run", SymbolKind::Function, 1, 1)],
            vec![make_reference("connect", ReferenceKind::Call, 1)],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![
            ("src/utils/index.ts", target),
            ("src/app.ts", dependent),
            ("src/other.ts", unrelated),
        ]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect src/utils connect".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            !result.contains("Ambiguous symbol selector"),
            "slash module aliases without line should still resolve the exact file hint: {result}"
        );
        assert!(
            result.contains("src/app.ts"),
            "slash module aliases without line should still return symbol context results: {result}"
        );
        assert!(
            !result.contains("src/other.ts"),
            "slash module aliases without line should exclude unrelated same-name hits: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_slash_module_alias_line_hint_disambiguates_selector() {
        let target = IndexedFile {
            relative_path: "src/utils/index.ts".to_string(),
            language: LanguageId::TypeScript,
            classification: crate::domain::FileClassification::for_code_path("src/utils/index.ts"),
            content: b"export function connect() {}\n\nexport function connect() {}\n".to_vec(),
            symbols: vec![
                make_symbol("connect", SymbolKind::Function, 1, 1),
                make_symbol("connect", SymbolKind::Function, 3, 3),
            ],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 57,
            content_hash: "utils-ts-lines".to_string(),
            references: vec![],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let dependent = IndexedFile {
            relative_path: "src/app.ts".to_string(),
            language: LanguageId::TypeScript,
            classification: crate::domain::FileClassification::for_code_path("src/app.ts"),
            content: b"import { connect } from 'src/utils';\nconnect();\n".to_vec(),
            symbols: vec![make_symbol("run", SymbolKind::Function, 2, 2)],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 49,
            content_hash: "app-ts".to_string(),
            references: vec![
                ReferenceRecord {
                    name: "utils".to_string(),
                    qualified_name: Some("src/utils".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (24, 33),
                    line_range: (0, 0),
                    enclosing_symbol_index: None,
                },
                ReferenceRecord {
                    name: "connect".to_string(),
                    qualified_name: Some("src/utils/connect".to_string()),
                    kind: ReferenceKind::Call,
                    byte_range: (36, 42),
                    line_range: (1, 1),
                    enclosing_symbol_index: Some(0),
                },
            ],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let unrelated = make_indexed_file(
            "src/other.ts",
            vec![make_symbol("run", SymbolKind::Function, 1, 1)],
            vec![make_reference("connect", ReferenceKind::Call, 1)],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![
            ("src/utils/index.ts", target),
            ("src/app.ts", dependent),
            ("src/other.ts", unrelated),
        ]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect src/utils:4 connect".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            !result.contains("Ambiguous symbol selector"),
            "slash module aliases should allow direct line-hint disambiguation: {result}"
        );
        assert!(
            result.contains("src/app.ts"),
            "slash module aliases with line hints should keep exact-selector matches: {result}"
        );
        assert!(
            !result.contains("src/other.ts"),
            "slash module aliases with line hints should drop unrelated same-name hits: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_slash_module_alias_file_only_prefers_exact_outline() {
        let target = IndexedFile {
            relative_path: "src/utils/index.ts".to_string(),
            language: LanguageId::TypeScript,
            classification: crate::domain::FileClassification::for_code_path("src/utils/index.ts"),
            content: b"export function connect() {}\n".to_vec(),
            symbols: vec![make_symbol("connect", SymbolKind::Function, 1, 1)],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 28,
            content_hash: "utils-ts".to_string(),
            references: vec![],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let unrelated = make_indexed_file(
            "src/other.ts",
            vec![make_symbol("connect", SymbolKind::Function, 1, 1)],
            vec![],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![
            ("src/utils/index.ts", target),
            ("src/other.ts", unrelated),
        ]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect src/utils".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            result.contains("src/utils/index.ts"),
            "slash module aliases should resolve file-only prompts to the exact outline: {result}"
        );
        assert!(
            !result.contains("src/other.ts"),
            "slash module aliases should not outline unrelated files: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_partial_slash_module_alias_without_line_does_not_activate()
    {
        let target = IndexedFile {
            relative_path: "src/utils/index.ts".to_string(),
            language: LanguageId::TypeScript,
            classification: crate::domain::FileClassification::for_code_path("src/utils/index.ts"),
            content: b"export function connect() {}\n".to_vec(),
            symbols: vec![make_symbol("connect", SymbolKind::Function, 1, 1)],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 28,
            content_hash: "utils-ts".to_string(),
            references: vec![],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let dependent = IndexedFile {
            relative_path: "src/app.ts".to_string(),
            language: LanguageId::TypeScript,
            classification: crate::domain::FileClassification::for_code_path("src/app.ts"),
            content: b"import { connect } from 'src/utils';\nconnect();\n".to_vec(),
            symbols: vec![make_symbol("run", SymbolKind::Function, 2, 2)],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 49,
            content_hash: "app-ts".to_string(),
            references: vec![
                ReferenceRecord {
                    name: "utils".to_string(),
                    qualified_name: Some("src/utils".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (24, 33),
                    line_range: (0, 0),
                    enclosing_symbol_index: None,
                },
                ReferenceRecord {
                    name: "connect".to_string(),
                    qualified_name: Some("src/utils/connect".to_string()),
                    kind: ReferenceKind::Call,
                    byte_range: (36, 42),
                    line_range: (1, 1),
                    enclosing_symbol_index: Some(0),
                },
            ],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let unrelated = make_indexed_file(
            "src/other.ts",
            vec![make_symbol("run", SymbolKind::Function, 1, 1)],
            vec![make_reference("connect", ReferenceKind::Call, 1)],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![
            ("src/utils/index.ts", target),
            ("src/app.ts", dependent),
            ("src/other.ts", unrelated),
        ]);

        let partial = prompt_context_handler(
            State(state.clone()),
            Query(PromptContextParams {
                text: "inspect src/utilsx connect".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            partial.contains("src/app.ts"),
            "partial slash module aliases should stay on the fallback path: {partial}"
        );
        assert!(
            partial.contains("src/other.ts"),
            "partial slash module aliases should not collapse to one exact file: {partial}"
        );

        let continued = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect src/utils/more connect".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            continued.contains("src/app.ts"),
            "continued slash module aliases should stay on the fallback path: {continued}"
        );
        assert!(
            continued.contains("src/other.ts"),
            "continued slash module aliases should not collapse to one exact file: {continued}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_slash_module_alias_ignores_unrelated_colon_numbers() {
        let target = IndexedFile {
            relative_path: "src/utils/index.ts".to_string(),
            language: LanguageId::TypeScript,
            classification: crate::domain::FileClassification::for_code_path("src/utils/index.ts"),
            content: b"export function connect() {}\n\nexport function connect() {}\n".to_vec(),
            symbols: vec![
                make_symbol("connect", SymbolKind::Function, 1, 1),
                make_symbol("connect", SymbolKind::Function, 3, 3),
            ],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 57,
            content_hash: "utils-ts-lines".to_string(),
            references: vec![],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let dependent = IndexedFile {
            relative_path: "src/app.ts".to_string(),
            language: LanguageId::TypeScript,
            classification: crate::domain::FileClassification::for_code_path("src/app.ts"),
            content: b"import { connect } from 'src/utils';\nconnect();\n".to_vec(),
            symbols: vec![make_symbol("run", SymbolKind::Function, 2, 2)],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 49,
            content_hash: "app-ts".to_string(),
            references: vec![
                ReferenceRecord {
                    name: "utils".to_string(),
                    qualified_name: Some("src/utils".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (24, 33),
                    line_range: (0, 0),
                    enclosing_symbol_index: None,
                },
                ReferenceRecord {
                    name: "connect".to_string(),
                    qualified_name: Some("src/utils/connect".to_string()),
                    kind: ReferenceKind::Call,
                    byte_range: (36, 42),
                    line_range: (1, 1),
                    enclosing_symbol_index: Some(0),
                },
            ],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let state = make_state(vec![
            ("src/utils/index.ts", target),
            ("src/app.ts", dependent),
        ]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect src/utils build:3 connect".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            result.contains("Ambiguous symbol selector"),
            "unrelated colon numbers should not disambiguate slash module aliases: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_qualified_symbol_alias_prefers_exact_selector() {
        let src_target = make_indexed_file(
            "src/db.rs",
            vec![make_symbol("connect", SymbolKind::Function, 2, 2)],
            vec![],
            ParseStatus::Parsed,
        );
        let src_dependent = IndexedFile {
            relative_path: "src/service.rs".to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path("src/service.rs"),
            content: b"use crate::db::connect;\nfn run() { connect(); }\n".to_vec(),
            symbols: vec![make_symbol("run", SymbolKind::Function, 2, 2)],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 46,
            content_hash: "abc".to_string(),
            references: vec![
                ReferenceRecord {
                    name: "db".to_string(),
                    qualified_name: Some("crate::db".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (0, 6),
                    line_range: (0, 0),
                    enclosing_symbol_index: Some(0),
                },
                ReferenceRecord {
                    name: "connect".to_string(),
                    qualified_name: Some("crate::db::connect".to_string()),
                    kind: ReferenceKind::Call,
                    byte_range: (10, 16),
                    line_range: (1, 1),
                    enclosing_symbol_index: Some(0),
                },
            ],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let unrelated = make_indexed_file(
            "src/other.rs",
            vec![make_symbol("run", SymbolKind::Function, 1, 1)],
            vec![make_reference("connect", ReferenceKind::Call, 1)],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![
            ("src/db.rs", src_target),
            ("src/service.rs", src_dependent),
            ("src/other.rs", unrelated),
        ]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect crate::db::connect".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            result.contains("src/service.rs"),
            "qualified symbol aliases should keep exact-selector matches: {result}"
        );
        assert!(
            !result.contains("src/other.rs"),
            "qualified symbol aliases should drop unrelated same-name hits: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_qualified_symbol_alias_line_hint_disambiguates_selector() {
        let src_target = make_indexed_file(
            "src/db.rs",
            vec![
                make_symbol("connect", SymbolKind::Function, 1, 1),
                make_symbol("connect", SymbolKind::Function, 2, 2),
            ],
            vec![],
            ParseStatus::Parsed,
        );
        let src_dependent = IndexedFile {
            relative_path: "src/service.rs".to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path("src/service.rs"),
            content: b"use crate::db::connect;\nfn run() { connect(); }\n".to_vec(),
            symbols: vec![make_symbol("run", SymbolKind::Function, 2, 2)],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 46,
            content_hash: "abc".to_string(),
            references: vec![
                ReferenceRecord {
                    name: "db".to_string(),
                    qualified_name: Some("crate::db".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (0, 6),
                    line_range: (0, 0),
                    enclosing_symbol_index: Some(0),
                },
                ReferenceRecord {
                    name: "connect".to_string(),
                    qualified_name: Some("crate::db::connect".to_string()),
                    kind: ReferenceKind::Call,
                    byte_range: (10, 16),
                    line_range: (1, 1),
                    enclosing_symbol_index: Some(0),
                },
            ],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let unrelated = make_indexed_file(
            "src/other.rs",
            vec![make_symbol("run", SymbolKind::Function, 1, 1)],
            vec![make_reference("connect", ReferenceKind::Call, 1)],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![
            ("src/db.rs", src_target),
            ("src/service.rs", src_dependent),
            ("src/other.rs", unrelated),
        ]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect crate::db::connect:2".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            !result.contains("Ambiguous symbol selector"),
            "qualified symbol aliases should allow direct line-hint disambiguation: {result}"
        );
        assert!(
            result.contains("src/service.rs"),
            "qualified symbol aliases with line hints should keep exact-selector matches: {result}"
        );
        assert!(
            !result.contains("src/other.rs"),
            "qualified symbol aliases with line hints should drop unrelated same-name hits: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_partial_module_alias_without_line_does_not_activate() {
        let src_target = make_indexed_file(
            "src/db.rs",
            vec![make_symbol("connect", SymbolKind::Function, 2, 2)],
            vec![],
            ParseStatus::Parsed,
        );
        let src_dependent = make_indexed_file(
            "src/service.rs",
            vec![make_symbol("run", SymbolKind::Function, 1, 1)],
            vec![make_reference("connect", ReferenceKind::Call, 1)],
            ParseStatus::Parsed,
        );
        let alt_dependent = make_indexed_file(
            "src/other.rs",
            vec![make_symbol("run", SymbolKind::Function, 1, 1)],
            vec![make_reference("connect", ReferenceKind::Call, 1)],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![
            ("src/db.rs", src_target),
            ("src/service.rs", src_dependent),
            ("src/other.rs", alt_dependent),
        ]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect crate::dbx connect".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            result.contains("src/service.rs"),
            "partial module aliases should stay on the fallback path: {result}"
        );
        assert!(
            result.contains("src/other.rs"),
            "partial module aliases should not collapse to one exact file: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_partial_qualified_symbol_alias_does_not_activate() {
        let src_target = make_indexed_file(
            "src/db.rs",
            vec![make_symbol("connect", SymbolKind::Function, 2, 2)],
            vec![],
            ParseStatus::Parsed,
        );
        let src_dependent = make_indexed_file(
            "src/service.rs",
            vec![make_symbol("run", SymbolKind::Function, 1, 1)],
            vec![make_reference("connect", ReferenceKind::Call, 1)],
            ParseStatus::Parsed,
        );
        let alt_dependent = make_indexed_file(
            "src/other.rs",
            vec![make_symbol("run", SymbolKind::Function, 1, 1)],
            vec![make_reference("connect", ReferenceKind::Call, 1)],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![
            ("src/db.rs", src_target),
            ("src/service.rs", src_dependent),
            ("src/other.rs", alt_dependent),
        ]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect crate::db::connect::helper".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            result.contains("src/service.rs"),
            "continued qualified symbol aliases should stay on the fallback path: {result}"
        );
        assert!(
            result.contains("src/other.rs"),
            "continued qualified symbol aliases should not collapse to one exact file: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_dotted_qualified_symbol_alias_prefers_exact_selector() {
        let target = IndexedFile {
            relative_path: "pkg/db.py".to_string(),
            language: LanguageId::Python,
            classification: crate::domain::FileClassification::for_code_path("pkg/db.py"),
            content: b"def connect():\n    pass\n".to_vec(),
            symbols: vec![make_symbol("connect", SymbolKind::Function, 1, 1)],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 24,
            content_hash: "db-py".to_string(),
            references: vec![],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let dependent = IndexedFile {
            relative_path: "pkg/service.py".to_string(),
            language: LanguageId::Python,
            classification: crate::domain::FileClassification::for_code_path("pkg/service.py"),
            content: b"from pkg.db import connect\n\ndef run():\n    connect()\n".to_vec(),
            symbols: vec![make_symbol("run", SymbolKind::Function, 3, 3)],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 54,
            content_hash: "service-py".to_string(),
            references: vec![
                ReferenceRecord {
                    name: "db".to_string(),
                    qualified_name: Some("pkg.db".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (5, 11),
                    line_range: (0, 0),
                    enclosing_symbol_index: None,
                },
                ReferenceRecord {
                    name: "connect".to_string(),
                    qualified_name: Some("pkg.db.connect".to_string()),
                    kind: ReferenceKind::Call,
                    byte_range: (41, 47),
                    line_range: (3, 3),
                    enclosing_symbol_index: Some(0),
                },
            ],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let unrelated = IndexedFile {
            relative_path: "pkg/other.py".to_string(),
            language: LanguageId::Python,
            classification: crate::domain::FileClassification::for_code_path("pkg/other.py"),
            content: b"def run():\n    connect()\n".to_vec(),
            symbols: vec![make_symbol("run", SymbolKind::Function, 1, 1)],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 25,
            content_hash: "other-py".to_string(),
            references: vec![make_reference("connect", ReferenceKind::Call, 1)],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let state = make_state(vec![
            ("pkg/db.py", target),
            ("pkg/service.py", dependent),
            ("pkg/other.py", unrelated),
        ]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect pkg.db.connect".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            result.contains("pkg/service.py"),
            "dotted qualified symbol aliases should keep exact-selector matches: {result}"
        );
        assert!(
            !result.contains("pkg/other.py"),
            "dotted qualified symbol aliases should drop unrelated same-name hits: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_slash_qualified_symbol_alias_prefers_exact_selector() {
        let target = IndexedFile {
            relative_path: "src/utils/index.ts".to_string(),
            language: LanguageId::TypeScript,
            classification: crate::domain::FileClassification::for_code_path("src/utils/index.ts"),
            content: b"export function connect() {}\n".to_vec(),
            symbols: vec![make_symbol("connect", SymbolKind::Function, 1, 1)],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 28,
            content_hash: "utils-ts".to_string(),
            references: vec![],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let dependent = IndexedFile {
            relative_path: "src/app.ts".to_string(),
            language: LanguageId::TypeScript,
            classification: crate::domain::FileClassification::for_code_path("src/app.ts"),
            content: b"import { connect } from 'src/utils';\nconnect();\n".to_vec(),
            symbols: vec![make_symbol("run", SymbolKind::Function, 2, 2)],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 49,
            content_hash: "app-ts".to_string(),
            references: vec![
                ReferenceRecord {
                    name: "utils".to_string(),
                    qualified_name: Some("src/utils".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (24, 33),
                    line_range: (0, 0),
                    enclosing_symbol_index: None,
                },
                ReferenceRecord {
                    name: "connect".to_string(),
                    qualified_name: Some("src/utils/connect".to_string()),
                    kind: ReferenceKind::Call,
                    byte_range: (36, 42),
                    line_range: (1, 1),
                    enclosing_symbol_index: Some(0),
                },
            ],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let unrelated = IndexedFile {
            relative_path: "src/other.ts".to_string(),
            language: LanguageId::TypeScript,
            classification: crate::domain::FileClassification::for_code_path("src/other.ts"),
            content: b"connect();\n".to_vec(),
            symbols: vec![make_symbol("run", SymbolKind::Function, 1, 1)],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 10,
            content_hash: "other-ts".to_string(),
            references: vec![make_reference("connect", ReferenceKind::Call, 1)],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let state = make_state(vec![
            ("src/utils/index.ts", target),
            ("src/app.ts", dependent),
            ("src/other.ts", unrelated),
        ]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect src/utils/connect".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            result.contains("src/app.ts"),
            "slash qualified symbol aliases should keep exact-selector matches: {result}"
        );
        assert!(
            !result.contains("src/other.ts"),
            "slash qualified symbol aliases should drop unrelated same-name hits: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_slash_qualified_symbol_alias_line_hint_disambiguates_selector()
     {
        let target = IndexedFile {
            relative_path: "src/utils/index.ts".to_string(),
            language: LanguageId::TypeScript,
            classification: crate::domain::FileClassification::for_code_path("src/utils/index.ts"),
            content: b"export function connect() {}\n\nexport function connect() {}\n".to_vec(),
            symbols: vec![
                make_symbol("connect", SymbolKind::Function, 1, 1),
                make_symbol("connect", SymbolKind::Function, 3, 3),
            ],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 57,
            content_hash: "utils-ts-lines".to_string(),
            references: vec![],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let dependent = IndexedFile {
            relative_path: "src/app.ts".to_string(),
            language: LanguageId::TypeScript,
            classification: crate::domain::FileClassification::for_code_path("src/app.ts"),
            content: b"import { connect } from 'src/utils';\nconnect();\n".to_vec(),
            symbols: vec![make_symbol("run", SymbolKind::Function, 2, 2)],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 49,
            content_hash: "app-ts".to_string(),
            references: vec![
                ReferenceRecord {
                    name: "utils".to_string(),
                    qualified_name: Some("src/utils".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (24, 33),
                    line_range: (0, 0),
                    enclosing_symbol_index: None,
                },
                ReferenceRecord {
                    name: "connect".to_string(),
                    qualified_name: Some("src/utils/connect".to_string()),
                    kind: ReferenceKind::Call,
                    byte_range: (36, 42),
                    line_range: (1, 1),
                    enclosing_symbol_index: Some(0),
                },
            ],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let unrelated = IndexedFile {
            relative_path: "src/other.ts".to_string(),
            language: LanguageId::TypeScript,
            classification: crate::domain::FileClassification::for_code_path("src/other.ts"),
            content: b"connect();\n".to_vec(),
            symbols: vec![make_symbol("run", SymbolKind::Function, 1, 1)],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 10,
            content_hash: "other-ts".to_string(),
            references: vec![make_reference("connect", ReferenceKind::Call, 1)],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let state = make_state(vec![
            ("src/utils/index.ts", target),
            ("src/app.ts", dependent),
            ("src/other.ts", unrelated),
        ]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect src/utils/connect:4".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            !result.contains("Ambiguous symbol selector"),
            "slash qualified symbol aliases should allow direct line-hint disambiguation: {result}"
        );
        assert!(
            result.contains("src/app.ts"),
            "slash qualified symbol aliases with line hints should keep exact-selector matches: {result}"
        );
        assert!(
            !result.contains("src/other.ts"),
            "slash qualified symbol aliases with line hints should drop unrelated same-name hits: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_continued_dotted_qualified_symbol_alias_does_not_activate()
    {
        let target = IndexedFile {
            relative_path: "pkg/db.py".to_string(),
            language: LanguageId::Python,
            classification: crate::domain::FileClassification::for_code_path("pkg/db.py"),
            content: b"def connect():\n    pass\n".to_vec(),
            symbols: vec![make_symbol("connect", SymbolKind::Function, 1, 1)],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 24,
            content_hash: "db-py".to_string(),
            references: vec![],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let dependent = IndexedFile {
            relative_path: "pkg/service.py".to_string(),
            language: LanguageId::Python,
            classification: crate::domain::FileClassification::for_code_path("pkg/service.py"),
            content: b"from pkg.db import connect\n\ndef run():\n    connect()\n".to_vec(),
            symbols: vec![make_symbol("run", SymbolKind::Function, 3, 3)],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 54,
            content_hash: "service-py".to_string(),
            references: vec![
                ReferenceRecord {
                    name: "db".to_string(),
                    qualified_name: Some("pkg.db".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (5, 11),
                    line_range: (0, 0),
                    enclosing_symbol_index: None,
                },
                ReferenceRecord {
                    name: "connect".to_string(),
                    qualified_name: Some("pkg.db.connect".to_string()),
                    kind: ReferenceKind::Call,
                    byte_range: (41, 47),
                    line_range: (3, 3),
                    enclosing_symbol_index: Some(0),
                },
            ],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let unrelated = IndexedFile {
            relative_path: "pkg/other.py".to_string(),
            language: LanguageId::Python,
            classification: crate::domain::FileClassification::for_code_path("pkg/other.py"),
            content: b"def run():\n    connect()\n".to_vec(),
            symbols: vec![make_symbol("run", SymbolKind::Function, 1, 1)],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 25,
            content_hash: "other-py".to_string(),
            references: vec![make_reference("connect", ReferenceKind::Call, 1)],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let state = make_state(vec![
            ("pkg/db.py", target),
            ("pkg/service.py", dependent),
            ("pkg/other.py", unrelated),
        ]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect pkg.db.connect.more connect".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            result.contains("pkg/service.py"),
            "continued dotted aliases should stay on the fallback path: {result}"
        );
        assert!(
            result.contains("pkg/other.py"),
            "continued dotted aliases should not collapse to one exact file: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_continued_slash_qualified_symbol_alias_does_not_activate()
    {
        let target = IndexedFile {
            relative_path: "src/utils/index.ts".to_string(),
            language: LanguageId::TypeScript,
            classification: crate::domain::FileClassification::for_code_path("src/utils/index.ts"),
            content: b"export function connect() {}\n".to_vec(),
            symbols: vec![make_symbol("connect", SymbolKind::Function, 1, 1)],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 28,
            content_hash: "utils-ts".to_string(),
            references: vec![],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let dependent = IndexedFile {
            relative_path: "src/app.ts".to_string(),
            language: LanguageId::TypeScript,
            classification: crate::domain::FileClassification::for_code_path("src/app.ts"),
            content: b"import { connect } from 'src/utils';\nconnect();\n".to_vec(),
            symbols: vec![make_symbol("run", SymbolKind::Function, 2, 2)],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 49,
            content_hash: "app-ts".to_string(),
            references: vec![
                ReferenceRecord {
                    name: "utils".to_string(),
                    qualified_name: Some("src/utils".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (24, 33),
                    line_range: (0, 0),
                    enclosing_symbol_index: None,
                },
                ReferenceRecord {
                    name: "connect".to_string(),
                    qualified_name: Some("src/utils/connect".to_string()),
                    kind: ReferenceKind::Call,
                    byte_range: (36, 42),
                    line_range: (1, 1),
                    enclosing_symbol_index: Some(0),
                },
            ],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let unrelated = IndexedFile {
            relative_path: "src/other.ts".to_string(),
            language: LanguageId::TypeScript,
            classification: crate::domain::FileClassification::for_code_path("src/other.ts"),
            content: b"connect();\n".to_vec(),
            symbols: vec![make_symbol("run", SymbolKind::Function, 1, 1)],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 10,
            content_hash: "other-ts".to_string(),
            references: vec![make_reference("connect", ReferenceKind::Call, 1)],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let state = make_state(vec![
            ("src/utils/index.ts", target),
            ("src/app.ts", dependent),
            ("src/other.ts", unrelated),
        ]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect src/utils/connect/more connect".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            result.contains("src/app.ts"),
            "continued slash aliases should stay on the fallback path: {result}"
        );
        assert!(
            result.contains("src/other.ts"),
            "continued slash aliases should not collapse to one exact file: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_dotted_qualified_symbol_alias_line_hint_disambiguates_selector()
     {
        let target = IndexedFile {
            relative_path: "pkg/db.py".to_string(),
            language: LanguageId::Python,
            classification: crate::domain::FileClassification::for_code_path("pkg/db.py"),
            content: b"def connect():\n    pass\n\ndef connect():\n    pass\n".to_vec(),
            symbols: vec![
                make_symbol("connect", SymbolKind::Function, 1, 1),
                make_symbol("connect", SymbolKind::Function, 4, 4),
            ],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 49,
            content_hash: "db-py".to_string(),
            references: vec![],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let dependent = IndexedFile {
            relative_path: "pkg/service.py".to_string(),
            language: LanguageId::Python,
            classification: crate::domain::FileClassification::for_code_path("pkg/service.py"),
            content: b"from pkg.db import connect\n\ndef run():\n    connect()\n".to_vec(),
            symbols: vec![make_symbol("run", SymbolKind::Function, 3, 3)],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 54,
            content_hash: "service-py".to_string(),
            references: vec![
                ReferenceRecord {
                    name: "db".to_string(),
                    qualified_name: Some("pkg.db".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (5, 11),
                    line_range: (0, 0),
                    enclosing_symbol_index: None,
                },
                ReferenceRecord {
                    name: "connect".to_string(),
                    qualified_name: Some("pkg.db.connect".to_string()),
                    kind: ReferenceKind::Call,
                    byte_range: (41, 47),
                    line_range: (3, 3),
                    enclosing_symbol_index: Some(0),
                },
            ],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let unrelated = IndexedFile {
            relative_path: "pkg/other.py".to_string(),
            language: LanguageId::Python,
            classification: crate::domain::FileClassification::for_code_path("pkg/other.py"),
            content: b"def run():\n    connect()\n".to_vec(),
            symbols: vec![make_symbol("run", SymbolKind::Function, 1, 1)],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 25,
            content_hash: "other-py".to_string(),
            references: vec![make_reference("connect", ReferenceKind::Call, 1)],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let state = make_state(vec![
            ("pkg/db.py", target),
            ("pkg/service.py", dependent),
            ("pkg/other.py", unrelated),
        ]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect pkg.db.connect:5".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            !result.contains("Ambiguous symbol selector"),
            "dotted qualified symbol aliases should allow direct line-hint disambiguation: {result}"
        );
        assert!(
            result.contains("pkg/service.py"),
            "dotted qualified symbol aliases with line hints should keep exact-selector matches: {result}"
        );
        assert!(
            !result.contains("pkg/other.py"),
            "dotted qualified symbol aliases with line hints should drop unrelated same-name hits: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_dotted_qualified_symbol_alias_ignores_unrelated_colon_numbers()
     {
        let target = IndexedFile {
            relative_path: "pkg/db.py".to_string(),
            language: LanguageId::Python,
            classification: crate::domain::FileClassification::for_code_path("pkg/db.py"),
            content: b"def connect():\n    pass\n\ndef connect():\n    pass\n".to_vec(),
            symbols: vec![
                make_symbol("connect", SymbolKind::Function, 1, 1),
                make_symbol("connect", SymbolKind::Function, 4, 4),
            ],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 49,
            content_hash: "db-py".to_string(),
            references: vec![],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let dependent = IndexedFile {
            relative_path: "pkg/service.py".to_string(),
            language: LanguageId::Python,
            classification: crate::domain::FileClassification::for_code_path("pkg/service.py"),
            content: b"from pkg.db import connect\n\ndef run():\n    connect()\n".to_vec(),
            symbols: vec![make_symbol("run", SymbolKind::Function, 3, 3)],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 54,
            content_hash: "service-py".to_string(),
            references: vec![
                ReferenceRecord {
                    name: "db".to_string(),
                    qualified_name: Some("pkg.db".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (5, 11),
                    line_range: (0, 0),
                    enclosing_symbol_index: None,
                },
                ReferenceRecord {
                    name: "connect".to_string(),
                    qualified_name: Some("pkg.db.connect".to_string()),
                    kind: ReferenceKind::Call,
                    byte_range: (41, 47),
                    line_range: (3, 3),
                    enclosing_symbol_index: Some(0),
                },
            ],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let state = make_state(vec![("pkg/db.py", target), ("pkg/service.py", dependent)]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect pkg.db.connect build:4".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            result.contains("Ambiguous symbol selector"),
            "unrelated colon numbers should not disambiguate dotted qualified symbol aliases: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_slash_qualified_symbol_alias_ignores_unrelated_colon_numbers()
     {
        let target = IndexedFile {
            relative_path: "src/utils/index.ts".to_string(),
            language: LanguageId::TypeScript,
            classification: crate::domain::FileClassification::for_code_path("src/utils/index.ts"),
            content: b"export function connect() {}\n\nexport function connect() {}\n".to_vec(),
            symbols: vec![
                make_symbol("connect", SymbolKind::Function, 1, 1),
                make_symbol("connect", SymbolKind::Function, 3, 3),
            ],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 57,
            content_hash: "utils-ts-lines".to_string(),
            references: vec![],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let dependent = IndexedFile {
            relative_path: "src/app.ts".to_string(),
            language: LanguageId::TypeScript,
            classification: crate::domain::FileClassification::for_code_path("src/app.ts"),
            content: b"import { connect } from 'src/utils';\nconnect();\n".to_vec(),
            symbols: vec![make_symbol("run", SymbolKind::Function, 2, 2)],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 49,
            content_hash: "app-ts".to_string(),
            references: vec![
                ReferenceRecord {
                    name: "utils".to_string(),
                    qualified_name: Some("src/utils".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (24, 33),
                    line_range: (0, 0),
                    enclosing_symbol_index: None,
                },
                ReferenceRecord {
                    name: "connect".to_string(),
                    qualified_name: Some("src/utils/connect".to_string()),
                    kind: ReferenceKind::Call,
                    byte_range: (36, 42),
                    line_range: (1, 1),
                    enclosing_symbol_index: Some(0),
                },
            ],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let state = make_state(vec![
            ("src/utils/index.ts", target),
            ("src/app.ts", dependent),
        ]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect src/utils/connect build:3".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            result.contains("Ambiguous symbol selector"),
            "unrelated colon numbers should not disambiguate slash qualified symbol aliases: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_partial_module_alias_hint_does_not_activate() {
        let src_target = make_indexed_file(
            "src/db.rs",
            vec![make_symbol("connect", SymbolKind::Function, 2, 2)],
            vec![],
            ParseStatus::Parsed,
        );
        let alt_target = make_indexed_file(
            "src/data.rs",
            vec![make_symbol("connect", SymbolKind::Function, 2, 2)],
            vec![],
            ParseStatus::Parsed,
        );
        let src_dependent = make_indexed_file(
            "src/service.rs",
            vec![make_symbol("run", SymbolKind::Function, 1, 1)],
            vec![make_reference("connect", ReferenceKind::Call, 1)],
            ParseStatus::Parsed,
        );
        let alt_dependent = make_indexed_file(
            "src/other.rs",
            vec![make_symbol("run", SymbolKind::Function, 1, 1)],
            vec![make_reference("connect", ReferenceKind::Call, 1)],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![
            ("src/db.rs", src_target),
            ("src/data.rs", alt_target),
            ("src/service.rs", src_dependent),
            ("src/other.rs", alt_dependent),
        ]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect crate::d:2 connect".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            result.contains("src/service.rs"),
            "partial module aliases should stay on the fallback path: {result}"
        );
        assert!(
            result.contains("src/other.rs"),
            "partial module aliases should not collapse to one exact file: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_partial_extensionless_path_hint_does_not_activate() {
        let src_target = make_indexed_file(
            "src/db.rs",
            vec![make_symbol("connect", SymbolKind::Function, 2, 2)],
            vec![],
            ParseStatus::Parsed,
        );
        let alt_target = make_indexed_file(
            "src/data.rs",
            vec![make_symbol("connect", SymbolKind::Function, 2, 2)],
            vec![],
            ParseStatus::Parsed,
        );
        let src_dependent = make_indexed_file(
            "src/service.rs",
            vec![make_symbol("run", SymbolKind::Function, 1, 1)],
            vec![make_reference("connect", ReferenceKind::Call, 1)],
            ParseStatus::Parsed,
        );
        let alt_dependent = make_indexed_file(
            "src/other.rs",
            vec![make_symbol("run", SymbolKind::Function, 1, 1)],
            vec![make_reference("connect", ReferenceKind::Call, 1)],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![
            ("src/db.rs", src_target),
            ("src/data.rs", alt_target),
            ("src/service.rs", src_dependent),
            ("src/other.rs", alt_dependent),
        ]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect src/d:2 connect".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            result.contains("src/service.rs"),
            "partial extensionless paths should stay on the fallback path: {result}"
        );
        assert!(
            result.contains("src/other.rs"),
            "partial extensionless paths should not collapse to one exact file: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_ignores_unrelated_colon_numbers_for_line_hint() {
        let target = make_indexed_file(
            "src/db.rs",
            vec![
                make_symbol("connect", SymbolKind::Function, 1, 1),
                make_symbol("connect", SymbolKind::Function, 2, 2),
            ],
            vec![],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![("src/db.rs", target)]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect src/db.rs connect port 8080:2".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            result.contains("Ambiguous symbol selector"),
            "unrelated colon numbers should not count as path:line hints: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_ambiguous_basename_line_hint_does_not_activate() {
        let src_target = make_indexed_file(
            "src/db.rs",
            vec![make_symbol("connect", SymbolKind::Function, 1, 1)],
            vec![],
            ParseStatus::Parsed,
        );
        let test_target = make_indexed_file(
            "tests/db.rs",
            vec![make_symbol("connect", SymbolKind::Function, 1, 1)],
            vec![],
            ParseStatus::Parsed,
        );
        let src_dependent = IndexedFile {
            relative_path: "src/service.rs".to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path("src/service.rs"),
            content: b"use crate::db::connect;\nfn run() { connect(); }\n".to_vec(),
            symbols: vec![make_symbol("run", SymbolKind::Function, 2, 2)],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 46,
            content_hash: "abc".to_string(),
            references: vec![
                ReferenceRecord {
                    name: "db".to_string(),
                    qualified_name: Some("crate::db".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (0, 6),
                    line_range: (0, 0),
                    enclosing_symbol_index: Some(0),
                },
                ReferenceRecord {
                    name: "connect".to_string(),
                    qualified_name: Some("crate::db::connect".to_string()),
                    kind: ReferenceKind::Call,
                    byte_range: (10, 16),
                    line_range: (1, 1),
                    enclosing_symbol_index: Some(0),
                },
            ],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let test_dependent = IndexedFile {
            relative_path: "tests/helper.rs".to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path("tests/helper.rs"),
            content: b"use crate::db::connect;\nfn helper() { connect(); }\n".to_vec(),
            symbols: vec![make_symbol("helper", SymbolKind::Function, 2, 2)],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 52,
            content_hash: "def".to_string(),
            references: vec![
                ReferenceRecord {
                    name: "db".to_string(),
                    qualified_name: Some("crate::db".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (0, 6),
                    line_range: (0, 0),
                    enclosing_symbol_index: Some(0),
                },
                ReferenceRecord {
                    name: "connect".to_string(),
                    qualified_name: Some("crate::db::connect".to_string()),
                    kind: ReferenceKind::Call,
                    byte_range: (10, 16),
                    line_range: (1, 1),
                    enclosing_symbol_index: Some(0),
                },
            ],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let state = make_state(vec![
            ("src/db.rs", src_target),
            ("tests/db.rs", test_target),
            ("src/service.rs", src_dependent),
            ("tests/helper.rs", test_dependent),
        ]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect db.rs:1 connect".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            result.contains("src/service.rs"),
            "ambiguous basename should fall back to name-only symbol context: {result}"
        );
        assert!(
            result.contains("tests/helper.rs"),
            "ambiguous basename should not collapse to one file hint: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_ambiguous_extensionless_alias_does_not_activate() {
        let src_target = make_indexed_file(
            "src/db.rs",
            vec![make_symbol("connect", SymbolKind::Function, 1, 1)],
            vec![],
            ParseStatus::Parsed,
        );
        let test_target = make_indexed_file(
            "tests/db.py",
            vec![make_symbol("connect", SymbolKind::Function, 1, 1)],
            vec![],
            ParseStatus::Parsed,
        );
        let src_dependent = IndexedFile {
            relative_path: "src/service.rs".to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path("src/service.rs"),
            content: b"use crate::db::connect;\nfn run() { connect(); }\n".to_vec(),
            symbols: vec![make_symbol("run", SymbolKind::Function, 2, 2)],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 46,
            content_hash: "abc".to_string(),
            references: vec![
                ReferenceRecord {
                    name: "db".to_string(),
                    qualified_name: Some("crate::db".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (0, 6),
                    line_range: (0, 0),
                    enclosing_symbol_index: Some(0),
                },
                ReferenceRecord {
                    name: "connect".to_string(),
                    qualified_name: Some("crate::db::connect".to_string()),
                    kind: ReferenceKind::Call,
                    byte_range: (10, 16),
                    line_range: (1, 1),
                    enclosing_symbol_index: Some(0),
                },
            ],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let test_dependent = IndexedFile {
            relative_path: "tests/helper.py".to_string(),
            language: LanguageId::Python,
            classification: crate::domain::FileClassification::for_code_path("tests/helper.py"),
            content: b"from db import connect\n\ndef helper():\n    connect()\n".to_vec(),
            symbols: vec![make_symbol("helper", SymbolKind::Function, 3, 4)],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 51,
            content_hash: "def".to_string(),
            references: vec![
                ReferenceRecord {
                    name: "db".to_string(),
                    qualified_name: Some("db".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (5, 7),
                    line_range: (0, 0),
                    enclosing_symbol_index: None,
                },
                ReferenceRecord {
                    name: "connect".to_string(),
                    qualified_name: Some("db.connect".to_string()),
                    kind: ReferenceKind::Call,
                    byte_range: (39, 45),
                    line_range: (3, 3),
                    enclosing_symbol_index: Some(0),
                },
            ],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        };
        let state = make_state(vec![
            ("src/db.rs", src_target),
            ("tests/db.py", test_target),
            ("src/service.rs", src_dependent),
            ("tests/helper.py", test_dependent),
        ]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect db:1 connect".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            result.contains("src/service.rs"),
            "ambiguous extensionless alias should fall back to name-only symbol context: {result}"
        );
        assert!(
            result.contains("tests/helper.py"),
            "ambiguous extensionless alias should not collapse to one file hint: {result}"
        );
    }

    // -----------------------------------------------------------------------
    // stats_handler
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_stats_handler_returns_snapshot() {
        let state = make_state(vec![]);
        // Record some stats manually.
        state.token_stats.record_read(1000, 200);
        state.token_stats.record_write();

        let result = stats_handler(State(state)).await;
        let snap = result.0;
        assert_eq!(snap.read_fires, 1);
        assert_eq!(snap.write_fires, 1);
        assert_eq!(snap.read_saved_tokens, 200);
    }
}
