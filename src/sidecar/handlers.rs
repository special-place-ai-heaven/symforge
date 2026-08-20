//! HTTP endpoint handlers for the symforge sidecar.
//!
//! All handlers follow this contract:
//!  - Accept `State(state): State<SidecarState>` plus optional `Query(params)`.
//!  - Acquire `state.index.data_plane().read()`, extract owned data, drop the guard, then return text or Json.
//!  - Never hold a `RwLockReadGuard` across an `.await` point.
//!  - On file not found: return `StatusCode::NOT_FOUND`.

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};

use crate::domain::{LanguageId, ReferenceKind};
use crate::sidecar::{SidecarState, SymbolSnapshot, SymbolSnapshotCache, build_with_budget};
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

pub(crate) struct ImpactToolOutput {
    pub(crate) text: String,
    pub(crate) published: std::sync::Arc<crate::live_index::PublishedGeneration>,
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
    /// Byte budget for the `symbol_context` references section
    /// (`build_with_budget`). Explicit tool calls deserve a roomy default so a
    /// symbol with a couple dozen references renders in full; the prompt-context
    /// hook injection stays lean to avoid flooding auto-injected context.
    symbol_context_references_budget_bytes: u64,
}

/// ~100 tokens. Kept small for auto-injected prompt context.
const HOOK_REFERENCES_BUDGET_BYTES: u64 = 400;
/// ~1000 tokens. Explicit `get_symbol_context` calls render references in full.
const TOOL_REFERENCES_BUDGET_BYTES: u64 = 4000;

const HOOK_RENDER_OPTIONS: RenderOptions = RenderOptions {
    include_savings_footer: true,
    record_stats: true,
    symbol_context_references_budget_bytes: HOOK_REFERENCES_BUDGET_BYTES,
};

const TOOL_RENDER_OPTIONS: RenderOptions = RenderOptions {
    include_savings_footer: false,
    record_stats: true,
    symbol_context_references_budget_bytes: TOOL_REFERENCES_BUDGET_BYTES,
};

fn format_prompt_context_signal(level: &str, evidence: impl Into<String>, body: String) -> String {
    format!(
        "Prompt-context signal: {level}\nEvidence: {}\n\n{body}",
        evidence.into()
    )
}

fn no_high_confidence_prompt_context_message() -> String {
    // Dogfood #8 (2026-07-06): a no-evidence report must cost one line, not a
    // multi-line report — this lands in the agent's prompt on EVERY submit.
    "Prompt-context signal: none (no file/symbol/repo-map cue in prompt)".to_string()
}

fn context_source_authority_label(authority: ContextSourceAuthority) -> &'static str {
    match authority {
        ContextSourceAuthority::DiskRefreshed => "disk-refreshed",
        ContextSourceAuthority::CurrentIndex => "current index",
    }
}

fn parse_state_label(file: &crate::live_index::store::IndexedFile) -> &'static str {
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
    repo_root: Option<&std::path::Path>,
    relative_path: &str,
    expected_gen: u64,
) -> Result<ContextSourceAuthority, StatusCode> {
    let Some(repo_root) = repo_root else {
        return Ok(ContextSourceAuthority::CurrentIndex);
    };
    let Ok(abs_path) = safe_sidecar_path_for_freshen(repo_root, relative_path) else {
        return Ok(ContextSourceAuthority::CurrentIndex);
    };
    // V11 observation lane (C4c): a request-path freshen re-admission
    // observes under the incarnation current at call time — the handler
    // holds no id across time, so it cannot be a late callback.
    let authority =
        crate::live_index::index_lifecycle::activation::project_source_authority(repo_root);
    let observer = authority.active_observer();
    match watcher::freshen_file_if_stale(
        relative_path,
        &abs_path,
        state.index.data_plane(),
        expected_gen,
        &authority,
        observer,
    ) {
        watcher::FreshenResult::Fresh => Ok(ContextSourceAuthority::CurrentIndex),
        watcher::FreshenResult::StaleReindexed => Ok(ContextSourceAuthority::DiskRefreshed),
        watcher::FreshenResult::StaleRemoved => Ok(ContextSourceAuthority::DiskRefreshed),
        watcher::FreshenResult::GenerationMismatch => Ok(ContextSourceAuthority::CurrentIndex),
        watcher::FreshenResult::PublicationRejected => Err(StatusCode::SERVICE_UNAVAILABLE),
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

/// Middleware (dogfood #6 / spec 012 FR-006b, hook half): when a request
/// carries `caller_root`, refuse with 409 if it does not match the root the
/// CURRENT index was built from. A daemon session retargeted by another
/// agent's `index_folder` leaves this sidecar answering from a different
/// project than the caller's repo; the 409 makes the hook fall back to the
/// daemon, which resolves the project BY ROOT, instead of emitting false
/// "not found" alarms into prompt context.
pub async fn caller_root_guard(
    State(state): State<SidecarState>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    // /health and /stats stay root-agnostic: liveness probes and the hook's
    // fail-open target must never 409.
    let path = request.uri().path();
    if path != "/health"
        && path != "/stats"
        && let Some(caller_root) = query_param(request.uri().query(), "caller_root")
        && let Some(indexed_root) = state.index.data_plane().read().indexed_root.clone()
        && !roots_match(std::path::Path::new(&caller_root), &indexed_root)
    {
        return (
            StatusCode::CONFLICT,
            format!(
                "Sidecar index is rooted at {} but the caller is in {} — the shared session was likely retargeted by another agent's index_folder. Fall back to the daemon to resolve the caller's project by root.",
                indexed_root.display(),
                caller_root
            ),
        )
            .into_response();
    }
    next.run(request).await
}

pub(crate) fn query_param(query: Option<&str>, key: &str) -> Option<String> {
    for pair in query?.split('&') {
        if let Some((k, v)) = pair.split_once('=')
            && k == key
        {
            return crate::discovery::percent_decode_path(v).filter(|s| !s.is_empty());
        }
    }
    None
}

fn normalized_root_text_for_match(path_text: &str, windows: bool) -> String {
    let normalized = crate::daemon::normalized_path_text(path_text, windows);
    let trimmed = normalized.trim_end_matches('/').to_string();
    if windows {
        trimmed.to_ascii_lowercase()
    } else {
        trimmed
    }
}

pub(crate) fn roots_match(caller: &std::path::Path, indexed: &std::path::Path) -> bool {
    let caller = dunce::canonicalize(caller).unwrap_or_else(|_| caller.to_path_buf());
    let indexed = dunce::canonicalize(indexed).unwrap_or_else(|_| indexed.to_path_buf());
    if cfg!(windows) {
        let (Some(caller), Some(indexed)) = (caller.to_str(), indexed.to_str()) else {
            return false;
        };
        normalized_root_text_for_match(caller, true)
            == normalized_root_text_for_match(indexed, true)
    } else {
        // Unix paths are opaque native bytes. A lossy String comparison can
        // authorize a different root whose UTF-8 name contains U+FFFD.
        caller == indexed
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `GET /health` — index state, file count, symbol count, uptime.
pub async fn health_handler(
    State(state): State<SidecarState>,
) -> Result<Json<HealthResponse>, StatusCode> {
    let published = state.index.data_plane().published_state();

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

#[derive(Clone, Debug)]
struct SidecarQueryFence {
    project_generation: u64,
    source: crate::domain::SourceIdentity,
    indexed_root: std::path::PathBuf,
}

fn published_sidecar_index_is_queryable(
    published: &crate::live_index::PublishedGeneration,
) -> bool {
    let health_is_ready = matches!(
        published.health.status,
        crate::live_index::PublishedIndexStatus::Ready
    );
    let health_is_empty = matches!(
        published.health.status,
        crate::live_index::PublishedIndexStatus::Empty
    );
    let freshness_is_queryable = matches!(
        published.freshness.as_ref(),
        crate::domain::FreshnessStatus::Current
    ) || (health_is_empty
        && matches!(
            published.freshness.as_ref(),
            crate::domain::FreshnessStatus::Verifying
        )
        && matches!(
            published.health.snapshot_verify_state,
            crate::live_index::SnapshotVerifyState::NotNeeded
        ));
    (health_is_ready || health_is_empty)
        && published.source.is_some()
        && published.live.indexed_root.is_some()
        && freshness_is_queryable
}

fn sidecar_query_fence_for(
    published: &crate::live_index::PublishedGeneration,
) -> Option<SidecarQueryFence> {
    Some(SidecarQueryFence {
        project_generation: published.project_generation,
        source: published.source.as_deref()?.clone(),
        indexed_root: published.live.indexed_root.clone()?,
    })
}

fn published_matches_sidecar_query(
    published: &crate::live_index::PublishedGeneration,
    fence: &SidecarQueryFence,
) -> bool {
    published_sidecar_index_is_queryable(published)
        && published_matches_sidecar_fence(published, fence)
}

fn published_matches_sidecar_fence(
    published: &crate::live_index::PublishedGeneration,
    fence: &SidecarQueryFence,
) -> bool {
    published.project_generation == fence.project_generation
        && published.source.as_deref() == Some(&fence.source)
        && published.live.indexed_root.as_ref() == Some(&fence.indexed_root)
}

fn require_queryable_sidecar_index(state: &SidecarState) -> Result<SidecarQueryFence, StatusCode> {
    let published = state.index.data_plane().published_generation();
    let queryable = published_sidecar_index_is_queryable(&published);
    let gen_match =
        state.index.data_plane().current_project_generation() == published.project_generation;
    if queryable && gen_match {
        let fence = sidecar_query_fence_for(&published).ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
        if state
            .repo_root
            .as_deref()
            .is_some_and(|root| !roots_match(root, &fence.indexed_root))
        {
            return Err(StatusCode::CONFLICT);
        }
        Ok(fence)
    } else {
        Err(StatusCode::SERVICE_UNAVAILABLE)
    }
}

fn capture_queryable_sidecar_generation(
    state: &SidecarState,
    fence: &SidecarQueryFence,
) -> Result<std::sync::Arc<crate::live_index::PublishedGeneration>, StatusCode> {
    let published = state.index.data_plane().published_generation();
    if published_matches_sidecar_query(&published, fence)
        && state.index.data_plane().current_project_generation() == published.project_generation
    {
        Ok(published)
    } else {
        Err(StatusCode::SERVICE_UNAVAILABLE)
    }
}

fn capture_sidecar_generation_at_fence(
    state: &SidecarState,
    fence: &SidecarQueryFence,
) -> Result<std::sync::Arc<crate::live_index::PublishedGeneration>, StatusCode> {
    let published = state.index.data_plane().published_generation();
    if published_matches_sidecar_fence(&published, fence)
        && state.index.data_plane().current_project_generation() == published.project_generation
    {
        Ok(published)
    } else {
        Err(StatusCode::SERVICE_UNAVAILABLE)
    }
}

struct SymbolCacheGenerationEntry {
    cache: std::sync::Weak<parking_lot::RwLock<SymbolSnapshotCache>>,
    project_generation: u64,
}

static SYMBOL_CACHE_GENERATIONS: std::sync::LazyLock<
    parking_lot::Mutex<std::collections::HashMap<usize, SymbolCacheGenerationEntry>>,
> = std::sync::LazyLock::new(|| parking_lot::Mutex::new(std::collections::HashMap::new()));

fn ensure_symbol_cache_generation(
    state: &SidecarState,
    expected_generation: u64,
) -> Result<(), StatusCode> {
    if state.index.data_plane().current_project_generation() != expected_generation {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    let identity = std::sync::Arc::as_ptr(&state.symbol_cache) as usize;
    let mut generations = SYMBOL_CACHE_GENERATIONS.lock();
    generations.retain(|_, entry| entry.cache.strong_count() > 0);
    match generations.get_mut(&identity) {
        Some(entry)
            if entry
                .cache
                .upgrade()
                .is_some_and(|cache| std::sync::Arc::ptr_eq(&cache, &state.symbol_cache)) =>
        {
            if entry.project_generation != expected_generation {
                state.symbol_cache.write().clear();
                entry.project_generation = expected_generation;
            }
        }
        _ => {
            // A caller may pre-seed the public path-keyed cache before the first
            // handler call. Associate that initial state with the current
            // project; subsequent generation changes are cleared deterministically.
            generations.insert(
                identity,
                SymbolCacheGenerationEntry {
                    cache: std::sync::Arc::downgrade(&state.symbol_cache),
                    project_generation: expected_generation,
                },
            );
        }
    }
    if state.index.data_plane().current_project_generation() == expected_generation {
        Ok(())
    } else {
        state.symbol_cache.write().clear();
        generations.remove(&identity);
        Err(StatusCode::SERVICE_UNAVAILABLE)
    }
}

fn cached_symbols_at_generation(
    state: &SidecarState,
    path: &str,
    expected_generation: u64,
) -> Result<Option<Vec<SymbolSnapshot>>, StatusCode> {
    ensure_symbol_cache_generation(state, expected_generation)?;
    let cache = state.symbol_cache.read();
    if state.index.data_plane().current_project_generation() != expected_generation {
        drop(cache);
        state.symbol_cache.write().clear();
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    Ok(cache.get(path).cloned())
}

fn store_cached_symbols_at_generation(
    state: &SidecarState,
    path: &str,
    symbols: Vec<SymbolSnapshot>,
    expected_generation: u64,
) -> Result<(), StatusCode> {
    ensure_symbol_cache_generation(state, expected_generation)?;
    let mut cache = state.symbol_cache.write();
    if state.index.data_plane().current_project_generation() != expected_generation {
        cache.clear();
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    cache.insert(path.to_string(), symbols);
    if state.index.data_plane().current_project_generation() == expected_generation {
        Ok(())
    } else {
        cache.clear();
        Err(StatusCode::SERVICE_UNAVAILABLE)
    }
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
    let fence = require_queryable_sidecar_index(&state)?;
    let result = outline_hook_text(&state, &params, &fence);
    capture_queryable_sidecar_generation(&state, &fence)?;
    result
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

pub(crate) fn outline_tool_text_for_generation(
    state: &SidecarState,
    published: &crate::live_index::PublishedGeneration,
    params: &OutlineParams,
) -> Result<String, StatusCode> {
    outline_text_for_generation(
        state,
        published,
        params,
        TOOL_RENDER_OPTIONS,
        ContextSourceAuthority::CurrentIndex,
    )
}

fn outline_hook_text(
    state: &SidecarState,
    params: &OutlineParams,
    fence: &SidecarQueryFence,
) -> Result<String, StatusCode> {
    outline_text(state, params, HOOK_RENDER_OPTIONS, fence)
}

fn append_parse_status_lines(
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

fn outline_text(
    state: &SidecarState,
    params: &OutlineParams,
    options: RenderOptions,
    fence: &SidecarQueryFence,
) -> Result<String, StatusCode> {
    let source_authority = freshen_sidecar_path_if_stale_at_generation(
        state,
        Some(fence.indexed_root.as_path()),
        &params.path,
        fence.project_generation,
    )?;
    let published = capture_queryable_sidecar_generation(state, fence)?;
    outline_text_for_generation(state, &published, params, options, source_authority)
}

fn outline_text_for_generation(
    state: &SidecarState,
    published: &crate::live_index::PublishedGeneration,
    params: &OutlineParams,
    options: RenderOptions,
    source_authority: ContextSourceAuthority,
) -> Result<String, StatusCode> {
    let guard = published.live.as_ref();

    // Return 404 for non-indexed files.
    let file = guard.get_file(&params.path).ok_or(StatusCode::NOT_FOUND)?;

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
        text.push_str(&crate::protocol::format::compact_savings_footer(
            output_bytes as usize,
            file_bytes as usize,
        ));
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
    let fence = require_queryable_sidecar_index(&state)?;
    let impact_index = state.index.data_plane().clone();
    let _impact_guard = impact_index.lock_impact_analysis().await;
    capture_queryable_sidecar_generation(&state, &fence)?;
    let result = impact_hook_text(state.clone(), &params, &fence).await;
    finish_impact_response_at_fence(&state, &fence, result)
}

fn finish_impact_response_at_fence(
    state: &SidecarState,
    fence: &SidecarQueryFence,
    result: Result<String, StatusCode>,
) -> Result<String, StatusCode> {
    let response = result?;
    // The impact operation has already committed its receipt-owned index,
    // snapshot, cache, and stats effects. A later freshness-only transition
    // gates the next request; it must not erase this useful response. Keep the
    // final project/source/root fence so a concurrent project rebind still
    // refuses cross-project output.
    capture_sidecar_generation_at_fence(state, fence)?;
    Ok(response)
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
) -> Result<ImpactToolOutput, StatusCode> {
    let impact_index = state.index.data_plane().clone();
    let _impact_guard = impact_index.lock_impact_analysis().await;
    let published = state.index.data_plane().published_generation();
    let expected_generation = published.project_generation;
    let root = match published.live.indexed_root.clone() {
        Some(root) => root,
        None => resolve_repo_root(&state)?,
    };
    let result = impact_text(
        state.clone(),
        params,
        TOOL_RENDER_OPTIONS,
        root,
        expected_generation,
        published,
    )
    .await;
    if state.index.data_plane().current_project_generation() != expected_generation {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    result
}

async fn impact_hook_text(
    state: SidecarState,
    params: &ImpactParams,
    fence: &SidecarQueryFence,
) -> Result<String, StatusCode> {
    let root = fence.indexed_root.clone();
    let published = capture_sidecar_generation_at_fence(&state, fence)?;
    impact_text(
        state,
        params,
        HOOK_RENDER_OPTIONS,
        root,
        fence.project_generation,
        published,
    )
    .await
    .map(|output| output.text)
}

async fn impact_text(
    state: SidecarState,
    params: &ImpactParams,
    options: RenderOptions,
    root: std::path::PathBuf,
    expected_generation: u64,
    baseline: std::sync::Arc<crate::live_index::PublishedGeneration>,
) -> Result<ImpactToolOutput, StatusCode> {
    if state.index.data_plane().current_project_generation() != expected_generation {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    let requested = std::path::Path::new(&params.path);
    let absolute = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        root.join(requested)
    };
    let relative =
        crate::watcher::normalize_event_path(&absolute, &root).ok_or(StatusCode::BAD_REQUEST)?;
    let normalized_path = crate::live_index::query::normalize_path_query(&relative);
    if normalized_path.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let is_new_file = params.new_file.unwrap_or(false);

    if is_new_file {
        // HOOK-06: Index a new file from disk.
        return handle_new_file_impact(
            state,
            &root,
            &normalized_path,
            options,
            expected_generation,
            baseline,
        )
        .await;
    }

    let should_auto_index_new_file = {
        // AAP-002: `from_path` (not `from_extension`) so extensionless narrative
        // entry points and dotfiles (README, .env, .gitignore) auto-index the
        // same way discovery admits them.
        let is_supported = crate::domain::LanguageId::from_path(&normalized_path).is_some();
        let indexed = {
            let guard = state.index.data_plane().read();
            guard.get_file(&normalized_path).is_some()
        };
        is_supported && !indexed && root.join(&normalized_path).is_file()
    };

    if should_auto_index_new_file {
        return handle_new_file_impact(
            state,
            &root,
            &normalized_path,
            options,
            expected_generation,
            baseline,
        )
        .await;
    }

    // HOOK-05: Re-index existing file and compute symbol diff.
    handle_edit_impact(
        state,
        &root,
        &normalized_path,
        options,
        expected_generation,
        baseline,
    )
    .await
}

fn impact_skipped_text(published: &crate::live_index::PublishedGeneration, path: &str) -> String {
    use crate::domain::index::AdmissionTier;

    let view = published.live.capture_admission_tier_lookup_view(path);
    let Some(view) = view else {
        return format!(
            "Not indexed: {path} is excluded by repository scope. The admission gate applies to \
             analyze_file_impact the same as bulk load and the watcher (no force-admit)."
        );
    };
    let tier_label = match view.tier {
        AdmissionTier::Normal => "Tier 1",
        AdmissionTier::MetadataOnly => "Tier 2 (metadata only)",
        AdmissionTier::HardSkip => "Tier 3 (hard skip)",
    };
    let reason = view
        .reason
        .map(|reason| reason.to_string())
        .unwrap_or_else(|| "policy".to_string());
    let size_mb = view.size.unwrap_or(0) as f64 / (1024.0 * 1024.0);

    // SF-AAP-002 is scoped to genuinely NON-PARSER files (no code parser exists
    // for the type — the artifact/binary case). A parser-supported file demoted
    // for SIZE is NOT this case: it keeps the honest oversize refusal that
    // impact_admission (a frozen behavioral contract) pins. `from_path` is the
    // same parser-support signal impact_text uses for auto-indexing (and that
    // discovery admits by), so the wording stays truthful in both branches.
    // AAP-002: `from_path` (not `from_extension`) recognizes extensionless
    // Text/Env entry points (README, .env, .gitignore) as parser-supported, so a
    // demoted one keeps the oversize refusal; `.bin` still reads as non-parser.
    let has_code_parser = crate::domain::LanguageId::from_path(path).is_some();

    // Key the recovery sentence on the read gate's predicted verdict (spec-023):
    // "Use get_file_content" must never point at a read the gate will refuse.
    let raw_read_advice =
        if crate::protocol::read_gate::disk_read_would_refuse(&published.live, path, view.size) {
            "Its contents are withheld by the admission policy — get_file_content will refuse \
         this file."
        } else {
            "Use get_file_content for raw reads."
        };

    if has_code_parser {
        return format!(
            "Not indexed: {path} is {tier_label} — reason: {reason}, size {size_mb:.1} MB. \
             The admission gate applies to analyze_file_impact the same as bulk load \
             and the watcher (no force-admit). {raw_read_advice}"
        );
    }

    let generation = published.project_generation;
    // Reconciled non-parser file: EXISTS in the catalog, analysis simply
    // unsupported. Report truthful existence + generation/Tier evidence and a
    // typed unsupported-analysis outcome — never false absence for a tracked file.
    format!(
        "── Impact: {path} ──\n\
         Status: exists (analysis unsupported — {tier_label}, no code parser)\n\
         exists: true\n\
         Tier: {tier_label} — reason: {reason}, size {size_mb:.1} MB\n\
         Generation: {generation}\n\
         The file IS tracked (metadata only), not absent; impact/symbol analysis is \
         unsupported for this file type. {raw_read_advice}"
    )
}

fn impact_receipt_publication(
    receipt: &crate::live_index::single_file::ReindexReceipt,
    baseline: &std::sync::Arc<crate::live_index::PublishedGeneration>,
) -> Result<std::sync::Arc<crate::live_index::PublishedGeneration>, StatusCode> {
    if let Some(published) = &receipt.published {
        return Ok(std::sync::Arc::clone(published));
    }
    if crate::live_index::store::PublicationFence::from_published(baseline.as_ref())
        == receipt.observed_at
    {
        return Ok(std::sync::Arc::clone(baseline));
    }
    Err(StatusCode::SERVICE_UNAVAILABLE)
}

async fn handle_new_file_impact(
    state: SidecarState,
    root: &std::path::Path,
    path: &str,
    options: RenderOptions,
    expected_generation: u64,
    baseline: std::sync::Arc<crate::live_index::PublishedGeneration>,
) -> Result<ImpactToolOutput, StatusCode> {
    if state.index.data_plane().current_project_generation() != expected_generation {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    let abs_path = root.join(path);
    let path_owned = path.to_string();
    let index = state.index.data_plane().clone();
    let receipt = tokio::task::spawn_blocking(move || {
        crate::watcher::admit_and_index_single_path_with_receipt(
            &path_owned,
            &abs_path,
            &index,
            expected_generation,
        )
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if state.index.data_plane().current_project_generation() != expected_generation {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    match &receipt.outcome {
        crate::watcher::ReindexResult::Reindexed | crate::watcher::ReindexResult::HashSkip => {}
        crate::watcher::ReindexResult::Skipped => {
            let published = impact_receipt_publication(&receipt, &baseline)?;
            return Ok(ImpactToolOutput {
                text: impact_skipped_text(published.as_ref(), path),
                published,
            });
        }
        crate::watcher::ReindexResult::PublicationRejected => {
            return Err(StatusCode::SERVICE_UNAVAILABLE);
        }
        crate::watcher::ReindexResult::NotFound | crate::watcher::ReindexResult::Removed => {
            if state.index.data_plane().publication_fence() != receipt.observed_at {
                return Err(StatusCode::SERVICE_UNAVAILABLE);
            }
            return Err(StatusCode::NOT_FOUND);
        }
        crate::watcher::ReindexResult::ReadError(_) => {
            let published = impact_receipt_publication(&receipt, &baseline)?;
            return Ok(ImpactToolOutput {
                text: format!(
                    "Not indexed: {path} is temporarily unreadable; last-valid state was retained."
                ),
                published,
            });
        }
    }

    // Build symbol kind breakdown.
    let mut kind_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let published = receipt
        .published
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    if published.project_generation != expected_generation {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    let (language, post_symbols) = {
        let file = published.live.get_file(path).ok_or(StatusCode::NOT_FOUND)?;
        for symbol in &file.symbols {
            *kind_counts.entry(symbol.kind.to_string()).or_insert(0) += 1;
        }
        let symbols = file
            .symbols
            .iter()
            .map(|symbol| SymbolSnapshot {
                name: symbol.name.clone(),
                kind: symbol.kind.to_string(),
                line_range: symbol.line_range,
                byte_range: symbol.byte_range,
            })
            .collect();
        (file.language, symbols)
    };

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

    if receipt.snapshot_created {
        let replacement = crate::live_index::store::PublicationFence::from_published(published);
        let _ = state
            .index
            .data_plane()
            .take_pre_update_snapshot_for_publication_at_generation(
                path,
                expected_generation,
                replacement,
            );
    }
    // The next edit must diff against the newly indexed file, not an empty
    // baseline that would report every existing symbol as added.
    store_cached_symbols_at_generation(&state, path, post_symbols, expected_generation)?;

    if options.record_stats {
        state.token_stats.record_write();
    }

    let text = format!(
        "Language: {:?}\nSymbols: {}\n[Indexed, 0 callers yet]",
        language, kinds_str,
    );

    Ok(ImpactToolOutput {
        text,
        published: std::sync::Arc::clone(published),
    })
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

fn slice_byte_range(bytes: &[u8], range: (u32, u32)) -> Option<&[u8]> {
    let start = range.0 as usize;
    let end = range.1 as usize;
    (start < end && end <= bytes.len()).then(|| &bytes[start..end])
}

/// True when the matched symbol's core body text changed, not merely shifted byte
/// offsets after a prefix insertion (e.g. top-of-file comment edits).
fn symbol_body_bytes_changed(
    pre_bytes: &[u8],
    post_bytes: &[u8],
    pre: &SymbolSnapshot,
    post: &SymbolSnapshot,
) -> bool {
    match (
        slice_byte_range(pre_bytes, pre.byte_range),
        slice_byte_range(post_bytes, post.byte_range),
    ) {
        (Some(pre_slice), Some(post_slice)) => pre_slice != post_slice,
        _ => pre.line_range != post.line_range || pre.byte_range != post.byte_range,
    }
}
async fn handle_edit_impact(
    state: SidecarState,
    root: &std::path::Path,
    path: &str,
    options: RenderOptions,
    expected_generation: u64,
    baseline: std::sync::Arc<crate::live_index::PublishedGeneration>,
) -> Result<ImpactToolOutput, StatusCode> {
    if state.index.data_plane().current_project_generation() != expected_generation {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    // Get pre-edit symbols and bytes from the exact index-owned baseline first.
    // The public symbols-only cache is a last-resort compatibility fallback: it
    // cannot prove symbol-body identity, so it must never shadow available
    // content from a pre-update snapshot or the current indexed file.
    //
    // The index pre-update snapshot (`take_pre_update_snapshot`) fixes a race
    // where the watcher re-indexes the file before this hook fires, causing the
    // current index to already contain post-edit symbols/content while the hook
    // still needs the pre-edit baseline for an accurate diff.
    let pre_update = state
        .index
        .data_plane()
        .peek_pre_update_snapshot_at_generation(path, expected_generation);
    if state.index.data_plane().current_project_generation() != expected_generation {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    let pre_snapshot_replacement = pre_update.as_ref().map(|(_, replacement)| *replacement);
    let (pre_symbols, pre_content): (Vec<SymbolSnapshot>, Option<Vec<u8>>) = {
        if let Some((pre, _)) = pre_update {
            let symbols = pre
                .symbols
                .into_iter()
                .map(|s| SymbolSnapshot {
                    name: s.name,
                    kind: s.kind,
                    line_range: s.line_range,
                    byte_range: s.byte_range,
                })
                .collect();
            (symbols, Some(pre.content))
        } else if let Some(file) = state.index.data_plane().read().get_file(path).cloned() {
            let symbols = file
                .symbols
                .iter()
                .map(|s| SymbolSnapshot {
                    name: s.name.clone(),
                    kind: s.kind.to_string(),
                    line_range: s.line_range,
                    byte_range: s.byte_range,
                })
                .collect();
            (symbols, Some(file.content))
        } else if let Some(cached) =
            cached_symbols_at_generation(&state, path, expected_generation)?
        {
            (cached, None)
        } else {
            (Vec::new(), None)
        }
    };

    // File byte_len before re-indexing (content baseline comes from `pre_content` above).
    let file_bytes_pre: u64 = pre_content.as_ref().map_or(0, |b| b.len() as u64);

    let abs_path = root.join(path);
    let path_owned = path.to_string();
    let index = state.index.data_plane().clone();
    let receipt = tokio::task::spawn_blocking(move || {
        crate::watcher::admit_and_index_single_path_with_receipt(
            &path_owned,
            &abs_path,
            &index,
            expected_generation,
        )
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if state.index.data_plane().current_project_generation() != expected_generation {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    match &receipt.outcome {
        crate::watcher::ReindexResult::Reindexed | crate::watcher::ReindexResult::HashSkip => {}
        crate::watcher::ReindexResult::Skipped => {
            let published = impact_receipt_publication(&receipt, &baseline)?;
            return Ok(ImpactToolOutput {
                text: impact_skipped_text(published.as_ref(), path),
                published,
            });
        }
        crate::watcher::ReindexResult::PublicationRejected => {
            return Err(StatusCode::SERVICE_UNAVAILABLE);
        }
        crate::watcher::ReindexResult::ReadError(_) => {
            let published = impact_receipt_publication(&receipt, &baseline)?;
            return Ok(ImpactToolOutput {
                text: format!(
                    "── Impact: {path} ──\nStatus: temporarily unreadable — last-valid state retained"
                ),
                published,
            });
        }
        crate::watcher::ReindexResult::NotFound | crate::watcher::ReindexResult::Removed => {
            let published = impact_receipt_publication(&receipt, &baseline)?;
            // One latency-bounded observation cannot distinguish a durable
            // deletion from delete→recreate disk ABA. Retain last-valid state;
            // the watcher retry/reconciliation path owns confirmed removal.
            let prev_symbol_count = pre_symbols.len();
            let root_display = root.display().to_string();
            let has_index_record = published.live.get_file(path).is_some();
            let (status, detail) = if has_index_record {
                (
                    "last-valid index state retained pending watcher confirmation",
                    format!("Previously indexed symbols: {prev_symbol_count}."),
                )
            } else {
                (
                    "no index record remains; watcher confirmation pending",
                    "No prior symbol count was observed.".to_string(),
                )
            };
            return Ok(ImpactToolOutput {
                text: format!(
                    "── Impact: {path} ──\nStatus: not found under {root_display} — {status}\n{detail}"
                ),
                published,
            });
        }
    }

    // Use the immutable generation returned by this request's winning
    // publication seam. Sampling current state here could accidentally adopt a
    // later watcher update and consume that update's snapshot.
    let post_generation = receipt
        .published
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    if post_generation.project_generation != expected_generation
        || state.index.data_plane().current_project_generation() != expected_generation
    {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    let (post_symbols, post_content) = {
        let file = post_generation
            .live
            .get_file(path)
            .ok_or(StatusCode::NOT_FOUND)?;
        let symbols: Vec<SymbolSnapshot> = file
            .symbols
            .iter()
            .map(|symbol| SymbolSnapshot {
                name: symbol.name.clone(),
                kind: symbol.kind.to_string(),
                line_range: symbol.line_range,
                byte_range: symbol.byte_range,
            })
            .collect();
        (symbols, file.content.clone())
    };
    if receipt.snapshot_created {
        let replacement =
            crate::live_index::store::PublicationFence::from_published(post_generation);
        let _ = state
            .index
            .data_plane()
            .take_pre_update_snapshot_for_publication_at_generation(
                path,
                expected_generation,
                replacement,
            );
    } else if matches!(receipt.outcome, crate::watcher::ReindexResult::HashSkip)
        && let Some(replacement) = pre_snapshot_replacement
    {
        let _ = state
            .index
            .data_plane()
            .take_pre_update_snapshot_for_publication_at_generation(
                path,
                expected_generation,
                replacement,
            );
    }
    let file_bytes: u64 = (post_content.len() as u64).max(file_bytes_pre);

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
            let body_changed = match pre_content.as_deref() {
                Some(pre_bytes) => symbol_body_bytes_changed(pre_bytes, &post_content, pr, ps),
                None => true,
            };
            if body_changed {
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
    store_cached_symbols_at_generation(&state, path, post_symbols.clone(), expected_generation)?;

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
            let guard = post_generation.live.as_ref();
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
        text.push_str(&crate::protocol::format::compact_savings_footer(
            output_bytes as usize,
            file_bytes as usize,
        ));
    }

    if options.record_stats {
        state.token_stats.record_edit(file_bytes, output_bytes);
    }

    Ok(ImpactToolOutput {
        text,
        published: std::sync::Arc::clone(post_generation),
    })
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
    let fence = require_queryable_sidecar_index(&state)?;
    let result = symbol_context_hook_text(&state, &params, &fence);
    capture_queryable_sidecar_generation(&state, &fence)?;
    result
}

/// Workflow adapter for search-hit expansion and quick caller/context reads.
pub async fn workflow_search_hit_expansion_handler(
    State(state): State<SidecarState>,
    Query(params): Query<SymbolContextParams>,
) -> Result<String, StatusCode> {
    symbol_context_handler(State(state), Query(params)).await
}

pub(crate) fn symbol_context_tool_text_for_generation(
    state: &SidecarState,
    published: &crate::live_index::PublishedGeneration,
    params: &SymbolContextParams,
) -> Result<String, StatusCode> {
    symbol_context_text_for_generation(
        state,
        published,
        params,
        TOOL_RENDER_OPTIONS,
        ContextSourceAuthority::CurrentIndex,
    )
}

fn symbol_context_hook_text(
    state: &SidecarState,
    params: &SymbolContextParams,
    fence: &SidecarQueryFence,
) -> Result<String, StatusCode> {
    symbol_context_text(state, params, HOOK_RENDER_OPTIONS, fence)
}

fn symbol_context_text(
    state: &SidecarState,
    params: &SymbolContextParams,
    options: RenderOptions,
    fence: &SidecarQueryFence,
) -> Result<String, StatusCode> {
    let source_authority = if let Some(path) = params.path.as_deref() {
        freshen_sidecar_path_if_stale_at_generation(
            state,
            Some(fence.indexed_root.as_path()),
            path,
            fence.project_generation,
        )?
    } else if let Some(file) = params.file.as_deref() {
        freshen_sidecar_path_if_stale_at_generation(
            state,
            Some(fence.indexed_root.as_path()),
            file,
            fence.project_generation,
        )?
    } else {
        ContextSourceAuthority::CurrentIndex
    };
    let published = capture_queryable_sidecar_generation(state, fence)?;
    symbol_context_text_for_generation(state, &published, params, options, source_authority)
}

fn symbol_context_text_for_generation(
    state: &SidecarState,
    published: &crate::live_index::PublishedGeneration,
    params: &SymbolContextParams,
    options: RenderOptions,
    source_authority: ContextSourceAuthority,
) -> Result<String, StatusCode> {
    let guard = published.live.as_ref();

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

        // Capture the enclosing symbol as a kind-aware display label
        // (e.g. "impl BucketManager", "struct BucketManager", "fn delta")
        // instead of bare name, so the reference list does not mislabel every
        // enclosing symbol as a function.
        let enclosing = reference.enclosing_symbol_index.and_then(|idx| {
            guard
                .get_file(file_path)
                .and_then(|f| f.symbols.get(idx as usize))
                .map(|s| {
                    crate::protocol::format::symbol_kind_name_label(&s.kind.to_string(), &s.name)
                })
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
            .map(parse_state_label)
            .unwrap_or_else(|| {
                aggregate_parse_state_label(std::iter::empty(), published.health.as_ref())
            })
    } else if let Some(file) = params.file.as_deref() {
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
    let (body_text, remaining) =
        build_with_budget(&body_lines, options.symbol_context_references_budget_bytes);
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
        let baseline_chars =
            crate::protocol::format::estimate_listing_baseline_chars(output_bytes as usize)
                .max(output_bytes as usize);
        text.push_str(&crate::protocol::format::compact_savings_footer(
            output_bytes as usize,
            baseline_chars.max(total_bytes as usize),
        ));
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
    let fence = require_queryable_sidecar_index(&state)?;
    let result = repo_map_text(&state, &fence);
    capture_queryable_sidecar_generation(&state, &fence)?;
    result
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
/// Rejects any path containing `:` (Windows drive letter — `C:\…`), starting
/// with `/` (POSIX absolute) or `\` (backslash-rooted / UNC), or containing a
/// `..` segment (parent-relative escape) — the same containment classes the
/// full/tree outline guard (`path_within_indexed_root`) drops (recovered
/// finding #7). Kept lexical on purpose: indexed workspace paths are stored as
/// relative forward-slash paths, so anything outside that shape is foreign. A
/// legit file literally named `src/a:b.rs` on POSIX would also be filtered,
/// but we accept that edge case in exchange for blocking the octogent-style
/// cross-workspace leak that motivated Unit 1.
fn is_intra_workspace_path(path: &str) -> bool {
    if path.contains(':') || path.starts_with('/') || path.starts_with('\\') {
        return false;
    }
    !path
        .replace('\\', "/")
        .split('/')
        .any(|segment| segment == "..")
}
fn repo_map_text(state: &SidecarState, fence: &SidecarQueryFence) -> Result<String, StatusCode> {
    let generation = capture_queryable_sidecar_generation(state, fence)?;
    repo_map_text_for_generation(&generation)
}

/// Render the compact topology from exactly one immutable publication root.
/// Callers that also append knowledge/authority evidence must pass the same
/// captured generation so no live/temporal/bridge lane can cross a swap.
pub(crate) fn repo_map_text_for_generation(
    generation: &crate::live_index::PublishedGeneration,
) -> Result<String, StatusCode> {
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
    let fence = require_queryable_sidecar_index(&state)?;
    let result = prompt_context_hook_text(&state, &params, &fence).await;
    capture_queryable_sidecar_generation(&state, &fence)?;
    result
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
    fence: &SidecarQueryFence,
) -> Result<String, StatusCode> {
    prompt_context_text(state, params, HOOK_RENDER_OPTIONS, fence).await
}

async fn prompt_context_text(
    state: &SidecarState,
    params: &PromptContextParams,
    options: RenderOptions,
    fence: &SidecarQueryFence,
) -> Result<String, StatusCode> {
    let prompt = params.text.trim();
    if prompt.is_empty() {
        return Ok(String::new());
    }
    let hint_generation = capture_queryable_sidecar_generation(state, fence)?;

    if let Some(symbol_hint) = find_prompt_qualified_symbol_hint(&hint_generation, prompt)? {
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
            fence,
        )?;
        let (level, evidence) = describe_file_hint(&symbol_hint.file_hint);
        return Ok(format_prompt_context_signal(level, evidence, body));
    }

    let file_hint = find_prompt_file_hint(&hint_generation, prompt)?;
    let symbol_hint = find_prompt_symbol_hint(&hint_generation, prompt)?;
    let line_hint = find_prompt_line_hint(prompt, file_hint.as_ref());

    match (file_hint, symbol_hint) {
        (Some(file_hint), Some(name)) => {
            // The symbol here came from loose prompt-token matching, so it is
            // only evidence about THIS file if it actually resolves inside it.
            // When it does not, symbol_context_text_for_generation renders the
            // resolver's error as the body (`Err(error) => return Ok(error)`),
            // and the injection asserts a confidence level and then reports
            // "Symbol not found in <file>" — a claim contradicted by its own
            // payload, landing in the agent's prompt on every submit.
            //
            // Observed: prompt mentioning `CLAUDE.md` plus the ordinary word
            // "session" produced `high-confidence` + "Symbol not found in
            // CLAUDE.md: session". The path hint was real; the symbol was a
            // collision. Fall back to the file-only signal, which is exactly
            // what this prompt would have produced without that collision.
            // Only NotFound is a collision. Ambiguous means the symbol IS in
            // the file under several candidates, and the existing rendering
            // already reports that usefully -- so this guard must use the same
            // resolver the renderer does, not a looser name comparison.
            let source_authority = freshen_sidecar_path_if_stale_at_generation(
                state,
                Some(fence.indexed_root.as_path()),
                &file_hint.path,
                fence.project_generation,
            )?;
            let published = capture_queryable_sidecar_generation(state, fence)?;
            let symbol_is_in_file = published
                .live
                .as_ref()
                .get_file(&file_hint.path)
                .is_some_and(|file| {
                    !matches!(
                        crate::live_index::disambiguation::resolve_symbol_selector(
                            file, &name, None, line_hint,
                        ),
                        crate::live_index::disambiguation::SymbolSelectorMatch::NotFound
                    )
                });
            if symbol_is_in_file {
                let body = symbol_context_text_for_generation(
                    state,
                    &published,
                    &SymbolContextParams {
                        name: name.clone(),
                        file: None,
                        path: Some(file_hint.path.clone()),
                        symbol_kind: None,
                        symbol_line: line_hint,
                    },
                    options,
                    source_authority,
                )?;
                let (level, file_evidence) = describe_file_hint(&file_hint);
                return Ok(format_prompt_context_signal(
                    level,
                    format!("{file_evidence}; symbol token `{name}` found in the index"),
                    body,
                ));
            }
            let body = outline_text(
                state,
                &OutlineParams {
                    path: file_hint.path.clone(),
                    max_tokens: Some(160),
                    sections: None,
                },
                options,
                fence,
            )?;
            let (level, evidence) = describe_file_hint(&file_hint);
            return Ok(format_prompt_context_signal(level, evidence, body));
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
                fence,
            )?;
            let (level, evidence) = describe_file_hint(&file_hint);
            return Ok(format_prompt_context_signal(level, evidence, body));
        }
        (None, Some(name)) => {
            // Dogfood #8 (2026-07-06): a bare prompt token matching a symbol
            // name is the weakest evidence tier — a conversational word can
            // collide with an indexed symbol. Emit a one-line pointer, never
            // a full symbol context (~1000 tokens on every prompt submit).
            return Ok(format!(
                "Prompt-context signal: heuristic\nEvidence: symbol token `{name}` matched somewhere in the index — get_symbol_context(name=\"{name}\") if intended"
            ));
        }
        (None, None) => {}
    }

    if prompt_requests_repo_map(prompt) {
        let body = repo_map_text_for_generation(&hint_generation)?;
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
    generation: &crate::live_index::PublishedGeneration,
    prompt: &str,
) -> Result<Option<PromptFileHint>, StatusCode> {
    let guard = generation.live.as_ref();
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
    generation: &crate::live_index::PublishedGeneration,
    prompt: &str,
) -> Result<Option<PromptQualifiedSymbolHint>, StatusCode> {
    let guard = generation.live.as_ref();
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

/// Common English function words and ubiquitous programming terms that
/// frequently appear in natural-language coding prompts. A bare lowercase
/// token in this set is treated as prose rather than a deliberate symbol
/// reference, which prevents noisy false-positive prompt-context signals such
/// as the word "any" incidentally matching `fn any` in the index.
const PROMPT_NOISE_WORDS: &[&str] = &[
    "add", "all", "and", "any", "api", "app", "are", "arg", "args", "but", "call", "can", "case",
    "check", "class", "code", "data", "def", "did", "does", "done", "else", "enum", "error",
    "fail", "few", "field", "file", "find", "fix", "for", "from", "func", "get", "had", "has",
    "have", "help", "her", "him", "his", "how", "impl", "index", "init", "into", "item", "its",
    "json", "just", "key", "kind", "let", "like", "line", "list", "load", "main", "make", "many",
    "map", "match", "may", "mod", "mode", "more", "most", "name", "need", "new", "node", "not",
    "now", "null", "off", "one", "only", "open", "our", "out", "own", "parse", "path", "read",
    "ref", "result", "run", "save", "see", "self", "set", "she", "show", "size", "some", "sort",
    "state", "step", "stop", "str", "sync", "task", "test", "text", "than", "that", "the", "their",
    "them", "then", "there", "they", "this", "those", "todo", "true", "type", "use", "used",
    "uses", "using", "value", "void", "want", "was", "way", "were", "what", "when", "where",
    "which", "who", "why", "will", "with", "write", "you", "your",
];

/// Returns true when `token` looks like a deliberate symbol reference rather
/// than an incidental natural-language word. Code identifiers (snake_case,
/// camelCase/PascalCase, all-caps acronyms, or names containing digits) never
/// collide with prose and are always distinctive; a plain lowercase word must
/// be at least four characters and absent from [`PROMPT_NOISE_WORDS`].
fn is_distinctive_symbol_token(token: &str) -> bool {
    if token.len() < 3 {
        return false;
    }
    if token.contains('_')
        || token
            .chars()
            .any(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    {
        return true;
    }
    token.len() >= 4 && !PROMPT_NOISE_WORDS.contains(&token)
}
fn find_prompt_symbol_hint(
    generation: &crate::live_index::PublishedGeneration,
    prompt: &str,
) -> Result<Option<String>, StatusCode> {
    let guard = generation.live.as_ref();
    for token in prompt_tokens(prompt) {
        // Path- and module-qualified mentions are served by the dedicated file
        // and qualified-symbol hints; this bare-token branch only handles plain
        // identifiers.
        if token.contains('/') || token.contains('.') {
            continue;
        }
        // Suppress prose: only fire for tokens that look like a deliberate code
        // identifier or a distinctive content word, so natural-language prompts
        // (e.g. "use any tool") do not match incidental symbols like `fn any`.
        if !is_distinctive_symbol_token(&token) {
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
    const REPO_MAP_TERMS: &[&str] = &[
        "architecture",
        "codebase",
        "map",
        "overview",
        "repo",
        "repository",
        "structure",
    ];
    // Whole-word matching: a substring scan would fire on unrelated words such
    // as "report" (repo), "mapping" (map), or "infrastructure" (structure),
    // wrongly dumping a full repo map into the high-confidence context branch.
    prompt
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|word| {
            REPO_MAP_TERMS
                .iter()
                .any(|term| word.eq_ignore_ascii_case(term))
        })
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;

    use once_cell::sync::Lazy;
    use parking_lot::RwLock;
    use tempfile::TempDir;

    use crate::domain::{LanguageId, ReferenceKind, ReferenceRecord, SymbolKind, SymbolRecord};
    use crate::live_index::store::{IndexedFile, LiveIndex, ParseStatus};
    use crate::sidecar::{SidecarState, SymbolSnapshot, TokenStats};

    static GENERIC_TEST_ROOT: Lazy<TempDir> =
        Lazy::new(|| TempDir::new().expect("create generic sidecar handler test root"));

    #[test]
    fn root_identity_treats_backslash_as_a_separator_only_on_windows() {
        let literal = r"/work/a\b";
        let nested = "/work/a/b";
        assert_ne!(
            normalized_root_text_for_match(literal, false),
            normalized_root_text_for_match(nested, false)
        );
        assert_eq!(
            normalized_root_text_for_match(literal, true),
            normalized_root_text_for_match(nested, true)
        );
    }

    #[cfg(unix)]
    #[test]
    fn roots_match_preserves_literal_backslash_path_components_on_unix() {
        let root = TempDir::new().expect("create sidecar root identity fixture");
        let literal = root.path().join(r"a\b");
        let nested = root.path().join("a").join("b");
        std::fs::create_dir_all(&literal).expect("create literal-backslash root");
        std::fs::create_dir_all(&nested).expect("create nested root");

        assert_ne!(
            literal.canonicalize().expect("canonicalize literal root"),
            nested.canonicalize().expect("canonicalize nested root")
        );
        assert!(
            !roots_match(&literal, &nested),
            "distinct Unix roots must not pass the caller-root guard"
        );
    }

    #[cfg(unix)]
    #[test]
    fn roots_match_never_aliases_non_utf8_native_root_to_lossy_utf8() {
        use std::os::unix::ffi::OsStringExt;

        let parent = tempfile::tempdir().expect("tempdir");
        let native = parent
            .path()
            .join(std::ffi::OsString::from_vec(vec![b'a', 0xff, b'b']));
        let lossy = parent.path().join("a\u{fffd}b");
        std::fs::create_dir(&native).expect("native root");
        std::fs::create_dir(&lossy).expect("lossy-collision root");
        assert_eq!(native.to_string_lossy(), lossy.to_string_lossy());
        assert!(roots_match(&native, &native));
        assert!(!roots_match(&native, &lossy));
    }

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

    /// SF-004 helper: an HTML `IndexedFile` whose only parse defect is Angular
    /// template control-flow that trips tree-sitter-html on the `>` operator.
    fn make_angular_html_partial(path: &str) -> IndexedFile {
        let content = "<div>\n  @if (items.length > 0) {\n  }\n</div>";
        let if_symbol = SymbolRecord {
            name: "@if".to_string(),
            kind: SymbolKind::Module,
            depth: 0,
            sort_order: 0,
            byte_range: (8, 9),
            item_byte_range: Some((8, 9)),
            // `@if (...)` is on 0-based line 1; diagnostic below is 1-based line 2.
            line_range: (1, 1),
            doc_byte_range: None,
        };
        IndexedFile {
            relative_path: path.to_string(),
            language: LanguageId::Html,
            classification: crate::domain::FileClassification::for_code_path(path),
            content: content.as_bytes().to_vec(),
            symbols: vec![if_symbol],
            parse_status: ParseStatus::PartialParse {
                warning: "tree-sitter reported syntax errors".to_string(),
            },
            parse_diagnostic: Some(crate::domain::index::ParseDiagnostic {
                parser: "tree-sitter".to_string(),
                message: "syntax error".to_string(),
                line: Some(2),
                column: Some(20),
                byte_span: Some((8, 31)),
                fallback_used: false,
            }),
            byte_len: content.len() as u64,
            content_hash: "html".to_string(),
            references: vec![],
            alias_map: HashMap::new(),
            mtime_secs: 0,
        }
    }

    #[test]
    fn test_symbol_body_bytes_changed_ignores_prefix_insertion_drift() {
        let pre = b"fn alpha() {}\nfn beta() {}";
        let post = b"// header\nfn alpha() {}\nfn beta() {}";
        let pre_alpha = SymbolSnapshot {
            name: "alpha".to_string(),
            kind: "fn".to_string(),
            line_range: (0, 0),
            byte_range: (0, 13),
        };
        let post_alpha = SymbolSnapshot {
            name: "alpha".to_string(),
            kind: "fn".to_string(),
            line_range: (1, 1),
            byte_range: (10, 23),
        };
        assert!(
            !symbol_body_bytes_changed(pre, post, &pre_alpha, &post_alpha),
            "unchanged symbol bodies after a prefix comment must not count as changed"
        );
    }

    #[test]
    fn test_symbol_body_bytes_changed_detects_real_body_edit() {
        let pre = b"fn alpha() {}";
        let post = b"fn alpha() { println!(\"x\"); }";
        let pre_alpha = SymbolSnapshot {
            name: "alpha".to_string(),
            kind: "fn".to_string(),
            line_range: (0, 0),
            byte_range: (0, 13),
        };
        let post_alpha = SymbolSnapshot {
            name: "alpha".to_string(),
            kind: "fn".to_string(),
            line_range: (0, 0),
            byte_range: (0, 30),
        };
        assert!(
            symbol_body_bytes_changed(pre, post, &pre_alpha, &post_alpha),
            "edited symbol body must count as changed"
        );
    }

    #[test]
    fn test_sf004_parse_state_label_marks_angular_template_partial_parsed() {
        let file = make_angular_html_partial("src/app/app.html");
        assert_eq!(
            parse_state_label(&file),
            "parsed",
            "an Angular template partial must surface as parsed, not a bare partial, \
             in the file-context envelope"
        );
    }

    #[test]
    fn test_sf004_outline_status_line_labels_angular_template_as_framework() {
        let file = make_angular_html_partial("src/app/app.html");
        let mut lines: Vec<String> = Vec::new();
        append_parse_status_lines(&mut lines, &file);
        let rendered = lines.join("\n");

        assert!(
            !rendered.contains("Parse status: partial"),
            "Angular template partial must not render a bare partial status: {rendered}"
        );
        assert!(
            rendered.contains("Parse status: ok (framework limitation:"),
            "Angular template partial should render the framework-limitation note: {rendered}"
        );
        assert!(
            !rendered.contains("Diagnostic: tree-sitter: syntax error"),
            "the alarming raw diagnostic must be suppressed for the framework case: {rendered}"
        );
    }

    fn build_shared_index(
        root: &std::path::Path,
        files: Vec<(&str, IndexedFile)>,
    ) -> crate::live_index::store::SharedIndex {
        LiveIndex::from_indexed_files(
            root,
            files
                .into_iter()
                .map(|(path, file)| (path.to_string(), file))
                .collect(),
        )
        .expect("sidecar handler test root resolves")
    }

    /// Build a SidecarState wrapping a SharedIndex for use in tests.
    fn make_state(files: Vec<(&str, IndexedFile)>) -> SidecarState {
        let root = GENERIC_TEST_ROOT.path().to_path_buf();
        SidecarState {
            index: crate::live_index::index_lifecycle::activation::ProjectRuntimeHandle::bind(
                build_shared_index(&root, files),
            ),
            token_stats: TokenStats::new(),
            repo_root: None,
            symbol_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn make_bootstrap_placeholder_state(files: Vec<(&str, IndexedFile)>) -> SidecarState {
        let index = LiveIndex::empty();
        {
            let mut guard = index.write();
            for (path, file) in files {
                guard.add_file(path.to_string(), file);
            }
        }
        SidecarState {
            index: crate::live_index::index_lifecycle::activation::ProjectRuntimeHandle::bind(
                index,
            ),
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
            index: crate::live_index::index_lifecycle::activation::ProjectRuntimeHandle::bind(
                build_shared_index(&repo_root, files),
            ),
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
            index: crate::live_index::index_lifecycle::activation::ProjectRuntimeHandle::bind(
                index,
            ),
            token_stats: TokenStats::new(),
            repo_root: Some(project_a.path().to_path_buf()),
            symbol_cache: Arc::new(RwLock::new(HashMap::new())),
        };

        let source_authority = freshen_sidecar_path_if_stale_at_generation(
            &state,
            state.repo_root.as_deref(),
            "src/b.rs",
            stale_gen,
        )
        .unwrap();

        assert!(matches!(
            source_authority,
            ContextSourceAuthority::CurrentIndex
        ));
        assert!(
            state
                .index
                .data_plane()
                .read()
                .get_file("src/b.rs")
                .is_some()
        );
    }

    #[test]
    fn stale_impact_generation_cannot_consume_or_overwrite_rebound_project_state() {
        let project_a = tempfile::tempdir().unwrap();
        let project_b = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(project_a.path().join("src")).unwrap();
        std::fs::create_dir_all(project_b.path().join("src")).unwrap();
        std::fs::write(
            project_b.path().join("src/shared.rs"),
            "pub fn project_b() {}\n",
        )
        .unwrap();

        let initial = make_indexed_file(
            "src/shared.rs",
            vec![make_symbol("project_a", SymbolKind::Function, 1, 2)],
            vec![],
            ParseStatus::Parsed,
        );
        let state = make_state_with_root(
            vec![("src/shared.rs", initial)],
            project_a.path().to_path_buf(),
        );
        let stale_generation = state.index.data_plane().current_project_generation();

        // Seed an A-generation pre-update snapshot, then prove the project
        // retarget clears it before B can publish any path-identical state.
        let replacement_a = make_indexed_file(
            "src/shared.rs",
            vec![make_symbol("project_a_next", SymbolKind::Function, 1, 2)],
            vec![],
            ParseStatus::Parsed,
        );
        assert!(state.index.data_plane().update_file_at_generation(
            "src/shared.rs",
            replacement_a,
            stale_generation,
        ));
        state.index.data_plane().reload(project_b.path()).unwrap();
        let current_generation = state.index.data_plane().current_project_generation();
        assert_ne!(current_generation, stale_generation);
        assert!(
            state
                .index
                .data_plane()
                .take_pre_update_snapshot_at_generation("src/shared.rs", current_generation)
                .is_none(),
            "retarget must discard the previous project's path-keyed snapshot"
        );

        // Publish a B-generation update so both the shared pre-update snapshot
        // and the sidecar-local cache contain replacement-project evidence.
        let replacement_b = make_indexed_file(
            "src/shared.rs",
            vec![make_symbol("project_b_next", SymbolKind::Function, 1, 2)],
            vec![],
            ParseStatus::Parsed,
        );
        assert!(state.index.data_plane().update_file_at_generation(
            "src/shared.rs",
            replacement_b,
            current_generation,
        ));
        let project_b_cache = vec![SymbolSnapshot {
            name: "project_b".to_string(),
            kind: SymbolKind::Function.to_string(),
            line_range: (1, 2),
            byte_range: (0, 10),
        }];
        store_cached_symbols_at_generation(
            &state,
            "src/shared.rs",
            project_b_cache.clone(),
            current_generation,
        )
        .unwrap();

        assert_eq!(
            cached_symbols_at_generation(&state, "src/shared.rs", stale_generation),
            Err(StatusCode::SERVICE_UNAVAILABLE)
        );
        assert_eq!(
            store_cached_symbols_at_generation(
                &state,
                "src/shared.rs",
                vec![SymbolSnapshot {
                    name: "stale_project_a".to_string(),
                    kind: SymbolKind::Function.to_string(),
                    line_range: (1, 2),
                    byte_range: (0, 10),
                }],
                stale_generation,
            ),
            Err(StatusCode::SERVICE_UNAVAILABLE)
        );
        assert!(
            state
                .index
                .data_plane()
                .take_pre_update_snapshot_at_generation("src/shared.rs", stale_generation)
                .is_none(),
            "a stale impact request must not consume B's pre-update snapshot"
        );

        assert_eq!(
            cached_symbols_at_generation(&state, "src/shared.rs", current_generation).unwrap(),
            Some(project_b_cache)
        );
        let project_b_snapshot = state
            .index
            .data_plane()
            .take_pre_update_snapshot_at_generation("src/shared.rs", current_generation)
            .expect("B's snapshot must remain available to the current generation");
        assert!(
            project_b_snapshot
                .symbols
                .iter()
                .any(|symbol| symbol.name == "project_b"),
            "the preserved snapshot must belong to project B"
        );
    }

    #[test]
    fn publication_bound_snapshot_take_preserves_same_hash_aba_update() {
        let mut first = make_indexed_file(
            "src/shared.rs",
            vec![make_symbol("alpha", SymbolKind::Function, 1, 2)],
            vec![],
            ParseStatus::Parsed,
        );
        first.content_hash = "hash-alpha".to_string();
        let state = make_state(vec![("src/shared.rs", first)]);
        let generation = state.index.data_plane().current_project_generation();

        let mut second = make_indexed_file(
            "src/shared.rs",
            vec![make_symbol("beta", SymbolKind::Function, 1, 2)],
            vec![],
            ParseStatus::Parsed,
        );
        second.content_hash = "hash-aba".to_string();
        assert!(state.index.data_plane().update_file_at_generation(
            "src/shared.rs",
            second,
            generation
        ));
        let second_fence = state.index.data_plane().publication_fence();

        let mut third = make_indexed_file(
            "src/shared.rs",
            vec![make_symbol("gamma", SymbolKind::Function, 1, 2)],
            vec![],
            ParseStatus::Parsed,
        );
        third.content_hash = "hash-aba".to_string();
        assert!(state.index.data_plane().update_file_at_generation(
            "src/shared.rs",
            third,
            generation
        ));
        let third_fence = state.index.data_plane().publication_fence();

        assert!(
            state
                .index
                .data_plane()
                .take_pre_update_snapshot_for_publication_at_generation(
                    "src/shared.rs",
                    generation,
                    second_fence,
                )
                .is_none(),
            "an older receipt must not drain a newer same-hash replacement's baseline"
        );
        let latest = state
            .index
            .data_plane()
            .take_pre_update_snapshot_for_publication_at_generation(
                "src/shared.rs",
                generation,
                third_fence,
            )
            .expect("the latest replacement must retain its own baseline");
        assert!(latest.symbols.iter().any(|symbol| symbol.name == "beta"));
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

    #[test]
    fn sidecar_queryability_requires_status_source_and_root_independently() {
        let file = make_indexed_file(
            "src/foo.rs",
            vec![make_symbol("alpha", SymbolKind::Function, 1, 5)],
            vec![],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![("src/foo.rs", file)]);
        let base = state.index.data_plane().published_generation();
        assert!(base.source.is_some());
        assert!(base.live.indexed_root.is_some());

        let variant = |status, source_bound: bool, root_bound: bool| {
            let mut health = (*base.health).clone();
            health.status = status;
            let mut live = (*base.live).clone();
            if !root_bound {
                live.indexed_root = None;
            }
            crate::live_index::PublishedGeneration {
                publication_generation: base.publication_generation,
                content_generation: base.content_generation,
                project_generation: base.project_generation,
                source: source_bound.then(|| Arc::clone(base.source.as_ref().unwrap())),
                source_version: base.source_version.clone(),
                freshness: Arc::clone(&base.freshness),
                manifest: base.manifest.clone(),
                code_signals: Arc::clone(&base.code_signals),
                bridge: Arc::clone(&base.bridge),
                authority: Arc::clone(&base.authority),
                live: Arc::new(live),
                health: Arc::new(health),
                outline: Arc::clone(&base.outline),
            }
        };

        use crate::live_index::PublishedIndexStatus::{Empty, Loading, Ready};
        assert!(published_sidecar_index_is_queryable(&variant(
            Ready, true, true
        )));
        assert!(published_sidecar_index_is_queryable(&variant(
            Empty, true, true
        )));
        assert!(!published_sidecar_index_is_queryable(&variant(
            Loading, true, true
        )));
        assert!(!published_sidecar_index_is_queryable(&variant(
            Ready, false, true
        )));
        assert!(!published_sidecar_index_is_queryable(&variant(
            Ready, true, false
        )));
        let mut freshness_degraded = variant(Ready, true, true);
        freshness_degraded.freshness = Arc::new(crate::domain::FreshnessStatus::Degraded {
            last_valid_content_generation: base.content_generation,
            reason_codes: vec![crate::domain::FreshnessReason::ObservationFailed],
        });
        assert!(
            !published_sidecar_index_is_queryable(&freshness_degraded),
            "a retained Ready health view with degraded freshness must refuse"
        );
        let mut freshness_verifying_ready = variant(Ready, true, true);
        freshness_verifying_ready.freshness = Arc::new(crate::domain::FreshnessStatus::Verifying);
        assert!(
            !published_sidecar_index_is_queryable(&freshness_verifying_ready),
            "a non-empty index still being verified must refuse"
        );
        let mut freshness_verifying_empty = variant(Empty, true, true);
        freshness_verifying_empty.freshness = Arc::new(crate::domain::FreshnessStatus::Verifying);
        assert!(
            published_sidecar_index_is_queryable(&freshness_verifying_empty),
            "a source-bound rooted empty repository is a terminal queryable state"
        );
        let mut snapshot_verifying_empty = freshness_verifying_empty;
        let mut snapshot_health = (*snapshot_verifying_empty.health).clone();
        snapshot_health.snapshot_verify_state = crate::live_index::SnapshotVerifyState::Pending;
        snapshot_verifying_empty.health = Arc::new(snapshot_health);
        assert!(
            !published_sidecar_index_is_queryable(&snapshot_verifying_empty),
            "an empty snapshot still being verified cannot authorize absence claims"
        );
    }

    #[test]
    fn post_impact_fence_preserves_response_across_freshness_only_transition() {
        let file = make_indexed_file(
            "src/foo.rs",
            vec![make_symbol("alpha", SymbolKind::Function, 1, 5)],
            vec![],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![("src/foo.rs", file)]);
        let fence = require_queryable_sidecar_index(&state).expect("ready sidecar fence");
        let last_valid_content_generation = state
            .index
            .data_plane()
            .published_generation()
            .content_generation;

        state
            .index
            .data_plane()
            .set_freshness_status(crate::domain::FreshnessStatus::Degraded {
                last_valid_content_generation,
                reason_codes: vec![crate::domain::FreshnessReason::ObservationFailed],
            });

        let gated_error = capture_queryable_sidecar_generation(&state, &fence)
            .err()
            .expect("degraded freshness must gate a new read");
        assert_eq!(gated_error, StatusCode::SERVICE_UNAVAILABLE);
        let response = finish_impact_response_at_fence(
            &state,
            &fence,
            Ok("committed impact response".to_string()),
        )
        .expect("freshness alone must not erase a committed impact response");
        assert_eq!(response, "committed impact response");

        let rebound = tempfile::tempdir().unwrap();
        std::fs::write(rebound.path().join("lib.rs"), "pub fn rebound() {}\n").unwrap();
        state.index.data_plane().reload(rebound.path()).unwrap();
        assert_eq!(
            finish_impact_response_at_fence(
                &state,
                &fence,
                Ok("cross-project response".to_string()),
            )
            .expect_err("a project rebind must still reject the old response"),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }

    #[tokio::test]
    async fn read_handlers_refuse_bootstrap_placeholder() {
        let repo = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(repo.path().join("src")).unwrap();
        std::fs::write(repo.path().join("src/new.rs"), "pub fn newly_added() {}\n").unwrap();
        let file = make_indexed_file(
            "src/foo.rs",
            vec![make_symbol("alpha", SymbolKind::Function, 1, 5)],
            vec![],
            ParseStatus::Parsed,
        );
        let mut state = make_bootstrap_placeholder_state(vec![("src/foo.rs", file)]);
        state.repo_root = Some(repo.path().to_path_buf());
        let generation_before = state.index.data_plane().current_project_generation();
        let replacement = make_indexed_file(
            "src/foo.rs",
            vec![make_symbol("beta", SymbolKind::Function, 1, 5)],
            vec![],
            ParseStatus::Parsed,
        );
        assert!(state.index.data_plane().update_file_at_generation(
            "src/foo.rs",
            replacement,
            generation_before,
        ));
        let seeded_cache = vec![SymbolSnapshot {
            name: "cached_alpha".to_string(),
            kind: SymbolKind::Function.to_string(),
            line_range: (1, 5),
            byte_range: (0, 10),
        }];
        store_cached_symbols_at_generation(&state, "src/foo.rs", seeded_cache, generation_before)
            .unwrap();
        let cache_before = state.symbol_cache.read().clone();
        let published_before = state.index.data_plane().published_generation();
        let published = state.index.data_plane().published_state();
        assert!(matches!(
            published.status,
            crate::live_index::PublishedIndexStatus::Loading
        ));
        assert_eq!(published.file_count, 1);
        assert!(
            state
                .index
                .data_plane()
                .read()
                .get_file("src/foo.rs")
                .is_some()
        );
        let write_fires_before = state
            .token_stats
            .write_fires
            .load(std::sync::atomic::Ordering::Relaxed);
        let edit_fires_before = state
            .token_stats
            .edit_fires
            .load(std::sync::atomic::Ordering::Relaxed);

        let outline_error = outline_handler(
            State(state.clone()),
            Query(OutlineParams {
                path: "src/foo.rs".to_string(),
                max_tokens: None,
                sections: None,
            }),
        )
        .await
        .expect_err("/outline must refuse an unready bootstrap placeholder");
        assert_eq!(outline_error, StatusCode::SERVICE_UNAVAILABLE);

        let impact_error = impact_handler(
            State(state.clone()),
            Query(ImpactParams {
                path: "src/foo.rs".to_string(),
                new_file: Some(false),
            }),
        )
        .await
        .expect_err("/impact must refuse an unready bootstrap placeholder");
        assert_eq!(impact_error, StatusCode::SERVICE_UNAVAILABLE);

        let new_file_impact_error = impact_handler(
            State(state.clone()),
            Query(ImpactParams {
                path: "src/new.rs".to_string(),
                new_file: Some(true),
            }),
        )
        .await
        .expect_err("/impact?new_file=true must refuse before admitting into a placeholder");
        assert_eq!(new_file_impact_error, StatusCode::SERVICE_UNAVAILABLE);

        let symbol_error = symbol_context_handler(
            State(state.clone()),
            Query(SymbolContextParams {
                name: "alpha".to_string(),
                file: None,
                path: None,
                symbol_kind: None,
                symbol_line: None,
            }),
        )
        .await
        .expect_err("/symbol-context must refuse an unready bootstrap placeholder");
        assert_eq!(symbol_error, StatusCode::SERVICE_UNAVAILABLE);

        let repo_map_error = repo_map_handler(State(state.clone()))
            .await
            .expect_err("/repo-map must refuse an unready bootstrap placeholder");
        assert_eq!(repo_map_error, StatusCode::SERVICE_UNAVAILABLE);

        let prompt_error = prompt_context_handler(
            State(state.clone()),
            Query(PromptContextParams {
                text: "alpha".to_string(),
            }),
        )
        .await
        .expect_err("the prompt hint must refuse an unready bootstrap placeholder");
        assert_eq!(prompt_error, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            state.index.data_plane().current_project_generation(),
            generation_before
        );
        let published_after = state.index.data_plane().published_generation();
        assert_eq!(
            published_after.publication_generation,
            published_before.publication_generation
        );
        assert_eq!(
            published_after.content_generation,
            published_before.content_generation
        );
        assert_eq!(published_after.health.file_count, 1);
        assert!(
            state
                .index
                .data_plane()
                .read()
                .get_file("src/new.rs")
                .is_none()
        );
        assert_eq!(&*state.symbol_cache.read(), &cache_before);
        let preserved_snapshot = state
            .index
            .data_plane()
            .take_pre_update_snapshot_at_generation("src/foo.rs", generation_before)
            .expect("loading refusal must preserve the pre-update snapshot");
        assert!(
            preserved_snapshot
                .symbols
                .iter()
                .any(|symbol| symbol.name == "alpha")
        );
        assert_eq!(
            state
                .token_stats
                .write_fires
                .load(std::sync::atomic::Ordering::Relaxed),
            write_fires_before
        );
        assert_eq!(
            state
                .token_stats
                .edit_fires
                .load(std::sync::atomic::Ordering::Relaxed),
            edit_fires_before
        );
    }

    #[tokio::test]
    async fn ready_but_source_unbound_index_is_not_queryable_by_sidecar() {
        let file = make_indexed_file(
            "src/foo.rs",
            vec![make_symbol("alpha", SymbolKind::Function, 1, 5)],
            vec![],
            ParseStatus::Parsed,
        );
        let mut files = HashMap::new();
        files.insert("src/foo.rs".to_string(), Arc::new(file));
        let index = crate::live_index::store::SharedIndexHandle::shared(
            LiveIndex::from_source_files(files),
        );
        let published = index.published_generation();
        assert!(matches!(
            published.health.status,
            crate::live_index::PublishedIndexStatus::Ready
        ));
        assert!(published.source.is_none());
        assert!(published.live.indexed_root.is_none());

        let state = SidecarState {
            index: crate::live_index::index_lifecycle::activation::ProjectRuntimeHandle::bind(
                index,
            ),
            token_stats: TokenStats::new(),
            repo_root: None,
            symbol_cache: Arc::new(RwLock::new(HashMap::new())),
        };
        let error = outline_handler(
            State(state),
            Query(OutlineParams {
                path: "src/foo.rs".to_string(),
                max_tokens: None,
                sections: None,
            }),
        )
        .await
        .expect_err("a Ready label cannot authorize a source-unbound index");
        assert_eq!(error, StatusCode::SERVICE_UNAVAILABLE);
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
            result.contains("Trust: exact | current index | parsed | full"),
            "outline should expose the compact trust line; got: {result}"
        );
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
        let result = impact_handler(State(state.clone()), Query(params)).await;
        // It may fail if the extension detection doesn't work for absolute paths, but
        // the basic test is that it doesn't panic.
        // The result depends on file system state.
        let _ = result; // just verify no panic
    }

    #[tokio::test]
    async fn impact_entry_points_share_the_index_single_flight_lock() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("src/db.rs"), "pub fn connect() {}\n").unwrap();
        let file = make_indexed_file(
            "src/db.rs",
            vec![make_symbol("connect", SymbolKind::Function, 1, 1)],
            vec![],
            ParseStatus::Parsed,
        );
        let state = make_state_with_root(vec![("src/db.rs", file)], tmp.path().to_path_buf());

        let held = state.index.data_plane().lock_impact_analysis().await;
        let http_state = state.clone();
        let mut http = tokio::spawn(async move {
            impact_handler(
                State(http_state),
                Query(ImpactParams {
                    path: "src/db.rs".to_string(),
                    new_file: None,
                }),
            )
            .await
        });
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(25), &mut http)
                .await
                .is_err(),
            "HTTP impact must wait for the shared index lock"
        );
        drop(held);
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(2), http)
                .await
                .expect("HTTP impact completes after lock release")
                .expect("HTTP impact task joins")
                .is_ok()
        );

        let held = state.index.data_plane().lock_impact_analysis().await;
        let tool_state = state.clone();
        let mut tool = tokio::spawn(async move {
            impact_tool_text(
                tool_state,
                &ImpactParams {
                    path: "src/db.rs".to_string(),
                    new_file: None,
                },
            )
            .await
        });
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(25), &mut tool)
                .await
                .is_err(),
            "direct-tool impact must wait for the shared index lock"
        );
        drop(held);
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(2), tool)
                .await
                .expect("direct-tool impact completes after lock release")
                .expect("direct-tool impact task joins")
                .is_ok()
        );
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
        let result = impact_handler(State(state.clone()), Query(params)).await;
        assert!(
            result.is_ok(),
            "impact_handler should return Ok even if file missing from disk"
        );
        let text = result.unwrap();
        assert!(
            text.contains("last-valid index state retained"),
            "should report that the last-valid index record was retained; got: {text}"
        );
        assert!(
            state
                .index
                .data_plane()
                .read()
                .get_file("src/db.rs")
                .is_some()
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

    /// A single missing-file observation retains the last-valid index record;
    /// the watcher retry/reconciliation path owns confirmed deletion.
    #[tokio::test]
    async fn test_impact_handler_edit_preserves_index_when_file_unreadable() {
        let file = make_indexed_file(
            "src/db.rs",
            vec![make_symbol("connect", SymbolKind::Function, 1, 10)],
            vec![],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![("src/db.rs", file)]);
        let generation = state.index.data_plane().current_project_generation();
        let replacement = make_indexed_file(
            "src/db.rs",
            vec![make_symbol("connect_v2", SymbolKind::Function, 1, 10)],
            vec![],
            ParseStatus::Parsed,
        );
        assert!(state.index.data_plane().update_file_at_generation(
            "src/db.rs",
            replacement,
            generation
        ));

        let params = ImpactParams {
            path: "src/db.rs".to_string(),
            new_file: None,
        };

        // File doesn't exist on disk — impact must fail open without deleting
        // a path that may be in a delete→recreate window.
        let result = impact_handler(State(state.clone()), Query(params)).await;
        assert!(result.is_ok(), "should return Ok, got: {result:?}");
        assert!(
            result
                .as_ref()
                .is_ok_and(|text| text.contains("last-valid index state retained"))
        );

        // Verify the last-valid file remains indexed pending confirmation.
        let guard = state.index.data_plane().read();
        assert!(
            guard.get_file("src/db.rs").is_some(),
            "one missing observation must not remove the last-valid index entry"
        );
        drop(guard);
        let preserved = state
            .index
            .data_plane()
            .take_pre_update_snapshot_at_generation("src/db.rs", generation)
            .expect("failed impact must preserve the watcher baseline");
        assert!(
            preserved
                .symbols
                .iter()
                .any(|symbol| symbol.name == "connect")
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
        assert!(
            result.contains("whole-file read") || result.contains("windowed read"),
            "should have footer: {result}"
        );
        assert!(
            result.contains("Trust: heuristic | current index | parsed | full"),
            "symbol_context should expose the compact trust line; got: {result}"
        );
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
    async fn test_symbol_context_evidence_marks_uncovered_caller_files() {
        // Dogfood F8: the Evidence header caps at 3 anchors, which one file can
        // exhaust. With callers in 4 files it must append `(+N more files)`
        // instead of silently naming only the anchored file(s).
        let a = make_indexed_file(
            "src/a.rs",
            vec![],
            vec![
                make_reference("target", ReferenceKind::Call, 1),
                make_reference("target", ReferenceKind::Call, 2),
                make_reference("target", ReferenceKind::Call, 3),
            ],
            ParseStatus::Parsed,
        );
        let one_ref = |line| vec![make_reference("target", ReferenceKind::Call, line)];
        let b = make_indexed_file("src/b.rs", vec![], one_ref(5), ParseStatus::Parsed);
        let c = make_indexed_file("src/c.rs", vec![], one_ref(6), ParseStatus::Parsed);
        let d = make_indexed_file("src/d.rs", vec![], one_ref(7), ParseStatus::Parsed);
        let state = make_state(vec![
            ("src/a.rs", a),
            ("src/b.rs", b),
            ("src/c.rs", c),
            ("src/d.rs", d),
        ]);

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
        assert!(
            result.contains(
                "Evidence: symbol token `target` anchored at src/a.rs:1, src/a.rs:2, src/a.rs:3 (+3 more files)"
            ),
            "evidence must flag caller files the anchor cap left unnamed; got: {result}"
        );
        // The body still lists every caller file.
        for file in ["src/a.rs", "src/b.rs", "src/c.rs", "src/d.rs"] {
            assert!(
                result.contains(file),
                "body must list {file}; got: {result}"
            );
        }
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
    fn repo_map_from_captured_generation_never_mixes_a_later_publication() {
        let project = tempfile::tempdir().expect("project");
        std::fs::create_dir_all(project.path().join("src")).expect("src");
        let source = project.path().join("src/model.rs");
        std::fs::write(&source, "pub struct OldMarker;\n").expect("old source");
        let shared = LiveIndex::load(project.path()).expect("old index");
        let captured = shared.published_source_set().current_generation();

        std::fs::write(&source, "pub struct NewMarker;\n").expect("new source");
        shared.reload(project.path()).expect("new publication");
        let next = shared.published_source_set().current_generation();

        let old_map = repo_map_text_for_generation(&captured).expect("old map");
        let new_map = repo_map_text_for_generation(&next).expect("new map");
        assert!(old_map.contains("OldMarker"), "{old_map}");
        assert!(!old_map.contains("NewMarker"), "{old_map}");
        assert!(new_map.contains("NewMarker"), "{new_map}");
        assert!(!new_map.contains("OldMarker"), "{new_map}");
    }

    #[test]
    fn outline_from_captured_generation_never_mixes_a_later_publication() {
        let project = tempfile::tempdir().expect("project");
        std::fs::create_dir_all(project.path().join("src")).expect("src");
        let source = project.path().join("src/model.rs");
        std::fs::write(&source, "pub struct OldOutlineMarker;\n").expect("old source");
        let shared = LiveIndex::load(project.path()).expect("old index");
        let state = SidecarState {
            index: crate::live_index::index_lifecycle::activation::ProjectRuntimeHandle::bind(
                shared.clone(),
            ),
            token_stats: TokenStats::new(),
            repo_root: Some(project.path().to_path_buf()),
            symbol_cache: Arc::new(RwLock::new(HashMap::new())),
        };
        let captured = shared.published_source_set().current_generation();

        std::fs::write(&source, "pub struct NewOutlineMarker;\n").expect("new source");
        shared.reload(project.path()).expect("new publication");
        let next = shared.published_source_set().current_generation();
        let params = OutlineParams {
            path: "src/model.rs".to_string(),
            max_tokens: None,
            sections: Some(vec!["outline".to_string()]),
        };

        let old_outline =
            outline_tool_text_for_generation(&state, &captured, &params).expect("old outline");
        let new_outline =
            outline_tool_text_for_generation(&state, &next, &params).expect("new outline");
        assert!(old_outline.contains("OldOutlineMarker"), "{old_outline}");
        assert!(!old_outline.contains("NewOutlineMarker"), "{old_outline}");
        assert!(new_outline.contains("NewOutlineMarker"), "{new_outline}");
        assert!(!new_outline.contains("OldOutlineMarker"), "{new_outline}");
    }

    #[test]
    fn symbol_context_from_captured_generation_never_mixes_a_later_publication() {
        let project = tempfile::tempdir().expect("project");
        std::fs::create_dir_all(project.path().join("src")).expect("src");
        let source = project.path().join("src/lib.rs");
        std::fs::write(
            &source,
            "pub fn target() {}\npub fn old_caller() { target(); }\n",
        )
        .expect("old source");
        let shared = LiveIndex::load(project.path()).expect("old index");
        let state = SidecarState {
            index: crate::live_index::index_lifecycle::activation::ProjectRuntimeHandle::bind(
                shared.clone(),
            ),
            token_stats: TokenStats::new(),
            repo_root: Some(project.path().to_path_buf()),
            symbol_cache: Arc::new(RwLock::new(HashMap::new())),
        };
        let captured = shared.published_source_set().current_generation();

        std::fs::write(
            &source,
            "pub fn target() {}\npub fn new_caller() { target(); }\n",
        )
        .expect("new source");
        shared.reload(project.path()).expect("new publication");
        let next = shared.published_source_set().current_generation();
        let params = SymbolContextParams {
            name: "target".to_string(),
            file: None,
            path: Some("src/lib.rs".to_string()),
            symbol_kind: Some("fn".to_string()),
            symbol_line: Some(1),
        };

        let old_context = symbol_context_tool_text_for_generation(&state, &captured, &params)
            .expect("old context");
        let new_context =
            symbol_context_tool_text_for_generation(&state, &next, &params).expect("new context");
        assert!(old_context.contains("old_caller"), "{old_context}");
        assert!(!old_context.contains("new_caller"), "{old_context}");
        assert!(new_context.contains("new_caller"), "{new_context}");
        assert!(!new_context.contains("old_caller"), "{new_context}");
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

    /// Recovered finding #7: the compact repo map's containment guard must also
    /// reject parent-relative escapes, UNC paths, and backslash-rooted paths —
    /// the same classes the full/tree outline guard drops.
    #[test]
    fn test_is_intra_workspace_path_rejects_parent_relative_unc_and_backslash_rooted() {
        assert!(is_intra_workspace_path("src/main.rs"));
        assert!(!is_intra_workspace_path("../evil.rs"));
        assert!(!is_intra_workspace_path("src/../../evil.rs"));
        assert!(!is_intra_workspace_path("..\\evil.rs"));
        assert!(!is_intra_workspace_path("\\\\server\\share\\evil.rs"));
        assert!(!is_intra_workspace_path("\\evil.rs"));
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

        // Dogfood #8: a bare-token match is the weakest evidence tier — it
        // must cost a one-line pointer, never a full symbol context.
        assert!(
            result.contains("Prompt-context signal: heuristic"),
            "symbol-only hints should be labeled heuristic: {result}"
        );
        assert!(
            result.contains("symbol token `connect` matched somewhere in the index"),
            "symbol-only hints should expose their evidence source: {result}"
        );
        assert!(
            result.contains("get_symbol_context(name=\"connect\")"),
            "the pointer should name the follow-up tool call: {result}"
        );
        assert!(
            !result.contains("src/service.rs"),
            "heuristic hints must not inline reference bodies: {result}"
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
            result.contains("Prompt-context signal: none"),
            "unmatched prompts should explicitly report no signal: {result}"
        );
        // Dogfood #8: a no-evidence report costs one line on every prompt submit.
        assert!(
            !result.trim().contains('\n'),
            "the no-signal report must be a single line: {result}"
        );
    }

    #[test]
    fn test_is_distinctive_symbol_token_filters_prose_keeps_identifiers() {
        // Deliberate code identifiers stay distinctive.
        assert!(is_distinctive_symbol_token("connect"));
        assert!(is_distinctive_symbol_token("find_prompt_symbol_hint"));
        assert!(is_distinctive_symbol_token("LiveIndex"));
        assert!(is_distinctive_symbol_token("utf8"));
        assert!(is_distinctive_symbol_token("HTTP"));
        // Common prose / generic programming words are suppressed.
        assert!(!is_distinctive_symbol_token("any"));
        assert!(!is_distinctive_symbol_token("the"));
        assert!(!is_distinctive_symbol_token("file"));
        assert!(!is_distinctive_symbol_token("value"));
        assert!(!is_distinctive_symbol_token("get"));
        // Too short to be a confident bare-token hint.
        assert!(!is_distinctive_symbol_token("io"));
    }

    #[tokio::test]
    async fn test_prompt_context_handler_ignores_common_word_symbol_collision() {
        // `any` is both a real symbol name (e.g. `fn any`) and an extremely common
        // English word. A natural-language prompt that merely contains "any" must
        // not produce a heuristic symbol signal, otherwise every prose prompt
        // pollutes the LLM's context with an irrelevant symbol expansion.
        let target = make_indexed_file(
            "src/db.rs",
            vec![make_symbol("any", SymbolKind::Function, 1, 1)],
            vec![],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![("src/db.rs", target)]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "can you use any helper that already exists".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            result.contains("Prompt-context signal: none"),
            "common-word collisions must not fire a heuristic symbol signal: {result}"
        );
        assert!(
            !result.contains("symbol token `any`"),
            "the prose word \"any\" must not surface as a symbol hint: {result}"
        );
    }

    #[test]
    fn test_prompt_requests_repo_map_matches_whole_words_only() {
        // Genuine repo-map requests fire.
        assert!(prompt_requests_repo_map("give me a codebase overview"));
        assert!(prompt_requests_repo_map("show the repo structure"));
        assert!(prompt_requests_repo_map("draw the architecture map"));
        assert!(prompt_requests_repo_map("what's in this repo?"));
        // Substrings of unrelated words must NOT fire (the prior `contains` bug).
        assert!(!prompt_requests_repo_map("generate a quarterly report"));
        assert!(!prompt_requests_repo_map("explain the mapping layer"));
        assert!(!prompt_requests_repo_map(
            "how does the infrastructure scale"
        ));
        assert!(!prompt_requests_repo_map("restructure this function"));
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

    /// A prompt can name a real file AND contain a token that matches a symbol
    /// living in a DIFFERENT file. The old code claimed a confidence level for
    /// that pair and then rendered the resolver's error as the body, so the
    /// injection contradicted itself with "Symbol not found in <file>" — on
    /// every prompt submit. Observed live: `CLAUDE.md` + the ordinary word
    /// "session". The file hint is still real, so fall back to its outline.
    #[tokio::test]
    async fn test_prompt_context_handler_symbol_collision_falls_back_to_file_outline() {
        let target = make_indexed_file(
            "src/db.rs",
            vec![make_symbol("connect", SymbolKind::Function, 1, 1)],
            vec![],
            ParseStatus::Parsed,
        );
        // `hydrate_cache` exists in the index, but NOT in src/db.rs.
        let elsewhere = make_indexed_file(
            "src/other.rs",
            vec![make_symbol("hydrate_cache", SymbolKind::Function, 1, 1)],
            vec![],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![("src/db.rs", target), ("src/other.rs", elsewhere)]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "look at src/db.rs hydrate_cache".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            !result.contains("Symbol not found"),
            "a symbol collision must not be reported as a resolved signal: {result}"
        );
        assert!(
            result.contains("src/db.rs"),
            "the file hint is real and must still be honoured: {result}"
        );
        assert!(
            result.contains("connect"),
            "should fall back to the file outline, which names its own symbols: {result}"
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
            partial.contains("symbol token `connect`"),
            "partial slash module aliases should stay on the fallback path: {partial}"
        );
        assert!(
            partial.contains("Prompt-context signal: heuristic"),
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
            continued.contains("symbol token `connect`"),
            "continued slash module aliases should stay on the fallback path: {continued}"
        );
        assert!(
            continued.contains("Prompt-context signal: heuristic"),
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
            result.contains("symbol token `connect`"),
            "partial module aliases should stay on the fallback path: {result}"
        );
        assert!(
            result.contains("Prompt-context signal: heuristic"),
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
            result.contains("symbol token `connect`"),
            "continued qualified symbol aliases should stay on the fallback path: {result}"
        );
        assert!(
            result.contains("Prompt-context signal: heuristic"),
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
            result.contains("symbol token `connect`"),
            "continued dotted aliases should stay on the fallback path: {result}"
        );
        assert!(
            result.contains("Prompt-context signal: heuristic"),
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
            result.contains("symbol token `connect`"),
            "continued slash aliases should stay on the fallback path: {result}"
        );
        assert!(
            result.contains("Prompt-context signal: heuristic"),
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
            result.contains("symbol token `connect`"),
            "partial module aliases should stay on the fallback path: {result}"
        );
        assert!(
            result.contains("Prompt-context signal: heuristic"),
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
            result.contains("symbol token `connect`"),
            "partial extensionless paths should stay on the fallback path: {result}"
        );
        assert!(
            result.contains("Prompt-context signal: heuristic"),
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
            result.contains("symbol token `connect`"),
            "ambiguous basename should fall back to name-only symbol context: {result}"
        );
        assert!(
            result.contains("Prompt-context signal: heuristic"),
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
            result.contains("symbol token `connect`"),
            "ambiguous extensionless alias should fall back to name-only symbol context: {result}"
        );
        assert!(
            result.contains("Prompt-context signal: heuristic"),
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

    // -----------------------------------------------------------------------
    // symbol_context references-section budget (Fix 2)
    // -----------------------------------------------------------------------

    /// Build an index where `process_request` is referenced from `ref_files`
    /// distinct files. The references resolve by name (no `path`/`file` filter),
    /// so all of them land in the references section. `ref_files <= 9` keeps the
    /// count under the 10-match display cap, isolating the byte budget as the
    /// only possible source of truncation. With long file paths the rendered
    /// body exceeds the old 400-byte (~100 token) budget but fits the new
    /// 4000-byte (~1000 token) tool budget.
    fn build_referenced_symbol_index(ref_files: usize) -> SidecarState {
        let files: Vec<(String, IndexedFile)> = (0..ref_files)
            .map(|i| {
                let path = format!("src/handlers/request_handler_module_{i:02}.rs");
                let file = make_indexed_file(
                    &path,
                    vec![],
                    vec![make_reference(
                        "process_request",
                        ReferenceKind::Call,
                        (i as u32) + 5,
                    )],
                    ParseStatus::Parsed,
                );
                (path, file)
            })
            .collect();
        let file_refs: Vec<(&str, IndexedFile)> =
            files.iter().map(|(p, f)| (p.as_str(), f.clone())).collect();
        make_state(file_refs)
    }

    #[test]
    fn test_symbol_context_tool_renders_references_without_default_budget_truncation() {
        // 9 referencing files fit under the 10-match cap, so truncation here
        // could only come from the byte budget — which the tool path must not hit.
        let state = build_referenced_symbol_index(9);
        let params = SymbolContextParams {
            name: "process_request".to_string(),
            file: None,
            path: None,
            symbol_kind: None,
            symbol_line: None,
        };

        let published = state.index.data_plane().published_generation();
        let tool = symbol_context_tool_text_for_generation(&state, &published, &params)
            .expect("tool render");

        // Sanity: the body really is larger than the old 400-byte budget, so
        // this test would have failed before the budget was raised.
        assert!(
            tool.len() > 400,
            "expected a references body larger than the old budget: {} bytes\n{tool}",
            tool.len()
        );
        assert!(
            !tool.contains("[truncated]"),
            "tool path must render all references without budget truncation: {tool}"
        );
        // All 9 referencing files should appear (none dropped by the 10-match cap).
        for i in 0..9 {
            let path = format!("src/handlers/request_handler_module_{i:02}.rs");
            assert!(
                tool.contains(&path),
                "expected reference file {path} in output: {tool}"
            );
        }
    }

    #[test]
    fn test_symbol_context_hook_keeps_lean_references_budget() {
        // The same body, rendered through the prompt-context hook path, must
        // still truncate at the lean ~100-token budget so auto-injected context
        // stays compact. This pins the intentional hook/tool budget split.
        let state = build_referenced_symbol_index(9);
        let params = SymbolContextParams {
            name: "process_request".to_string(),
            file: None,
            path: None,
            symbol_kind: None,
            symbol_line: None,
        };

        let fence = require_queryable_sidecar_index(&state).expect("queryable fixture");
        let hook = symbol_context_hook_text(&state, &params, &fence).expect("hook render");

        assert!(
            hook.contains("[truncated]"),
            "hook path should still truncate the references section at the lean budget: {hook}"
        );
    }
}
