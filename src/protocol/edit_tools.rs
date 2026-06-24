use std::path::{Path, PathBuf};
use std::time::Instant;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};
use serde::Serialize;

use crate::edit_safety::trust::{ProjectConfigTrust, TrustEvaluation, TrustStatus};
use crate::live_index::store::IndexState;
use crate::protocol::result_status::{
    OutcomeClass, RESULT_STATUS_CONTRACT_VERSION, RESULT_STATUS_META_KEY,
};
use crate::protocol::{edit, edit_format, edit_hooks, format};
use crate::watcher;

use super::SymForgeServer;
use super::tools::safe_repo_path_for_freshen;

macro_rules! loading_guard {
    ($guard:expr) => {
        match $guard.index_state() {
            IndexState::Ready => {}
            IndexState::Empty => return format::empty_guard_message(),
            IndexState::Loading => return format::loading_guard_message(),
            IndexState::CircuitBreakerTripped { summary } => {
                return format!("Index degraded: {summary}");
            }
        }
    };
}

const PROJECT_CONFIG_TRUST_MODE_ENV: &str = "SYMFORGE_PROJECT_CONFIG_TRUST_MODE";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProjectConfigTrustMode {
    LogOnly,
    Enforce,
}

impl ProjectConfigTrustMode {
    fn current() -> Self {
        match std::env::var(PROJECT_CONFIG_TRUST_MODE_ENV) {
            Ok(value) if value.eq_ignore_ascii_case("enforce") => Self::Enforce,
            _ => Self::LogOnly,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{EditResultStatus, classify_edit_output};

    #[test]
    fn daemon_wrapped_edit_errors_classify_as_invalid_request() {
        assert_eq!(
            classify_edit_output(
                "Error in replace_symbol_body: projects must not be empty",
                false
            ),
            EditResultStatus::InvalidRequest
        );
    }
}

fn project_config_trust_inputs_exist(repo_root: &Path) -> bool {
    let symforge_dir = repo_root.join(".symforge");
    symforge_dir.join("config.toml").exists() || symforge_dir.join("config").exists()
}

fn project_config_trust_response_suffix(repo_root: &Path) -> Result<Option<String>, String> {
    if !project_config_trust_inputs_exist(repo_root) {
        return Ok(None);
    }
    let Some(trust) = ProjectConfigTrust::default_store() else {
        return Ok(Some(
            "ProjectConfigTrustWarning: status=Unavailable warning=\"could not determine user-local data directory\"; mode=LOG_ONLY; operation_allowed=true"
                .to_string(),
        ));
    };
    let evaluation = trust.evaluate(repo_root);
    match evaluation.status {
        TrustStatus::Trusted | TrustStatus::EnvOverride => Ok(None),
        TrustStatus::Untrusted | TrustStatus::ContentChanged { .. } => {
            let evidence = project_config_trust_evidence(&evaluation);
            match ProjectConfigTrustMode::current() {
                ProjectConfigTrustMode::LogOnly => Ok(Some(format!(
                    "ProjectConfigTrustWarning: {evidence}; mode=LOG_ONLY; operation_allowed=true"
                ))),
                ProjectConfigTrustMode::Enforce => Err(format!(
                    "ProjectConfigTrustEnforced: {evidence}; mode=ENFORCE; operation_allowed=false; run `symforge trust project-config accept --project {}` with reviewed actual_hash before retrying",
                    repo_root.display()
                )),
            }
        }
    }
}

fn project_config_trust_evidence(evaluation: &TrustEvaluation) -> String {
    let mut parts = match &evaluation.status {
        TrustStatus::Trusted => vec!["status=Trusted".to_string()],
        TrustStatus::Untrusted => vec!["status=Untrusted".to_string()],
        TrustStatus::ContentChanged { expected, actual } => vec![
            "status=ContentChanged".to_string(),
            format!("expected_hash={expected}"),
            format!("actual_hash={actual}"),
        ],
        TrustStatus::EnvOverride => vec!["status=EnvOverride".to_string()],
    };
    if !matches!(evaluation.status, TrustStatus::ContentChanged { .. }) {
        parts.push(format!("actual_hash={}", evaluation.actual_hash));
    }
    if let Some(project_key) = &evaluation.project_key {
        parts.push(format!("project_key={project_key}"));
    }
    if let Some(warning) = evaluation.warnings.first() {
        parts.push(format!("warning=\"{}\"", one_line(warning)));
    }
    parts.join(" ")
}

fn append_project_config_trust_suffix(output: &mut String, suffix: Option<&str>) {
    if let Some(suffix) = suffix {
        output.push('\n');
        output.push_str(suffix);
    }
}

fn one_line(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditResultStatus {
    Success,
    DryRunSuccess,
    NotFound,
    Ambiguous,
    InvalidRequest,
    InternalFailure,
}

impl EditResultStatus {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::DryRunSuccess => "dry_run_success",
            Self::NotFound => "not_found",
            Self::Ambiguous => "ambiguous",
            Self::InvalidRequest => "invalid_request",
            Self::InternalFailure => "internal_failure",
        }
    }

    const fn outcome_class(self) -> OutcomeClass {
        match self {
            Self::Success | Self::DryRunSuccess => OutcomeClass::Found,
            Self::NotFound => OutcomeClass::NotFound,
            Self::Ambiguous => OutcomeClass::Ambiguous,
            Self::InvalidRequest => OutcomeClass::InvalidRequest,
            Self::InternalFailure => OutcomeClass::InternalFailure,
        }
    }
}

fn statused_edit_tool_result(
    text: String,
    status: EditResultStatus,
    operation_statuses: Vec<(usize, EditResultStatus)>,
) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
    let mut status_payload = serde_json::json!({
        "contract_version": RESULT_STATUS_CONTRACT_VERSION,
        "outcome_class": status.outcome_class().as_str(),
        "status": status.as_str(),
    });
    if !operation_statuses.is_empty() {
        status_payload["operations"] = serde_json::Value::Array(
            operation_statuses
                .into_iter()
                .map(|(operation_index, operation_status)| {
                    serde_json::json!({
                        "operation_index": operation_index,
                        "status": operation_status.as_str(),
                        "outcome_class": operation_status.outcome_class().as_str(),
                    })
                })
                .collect(),
        );
    }

    let mut meta = rmcp::model::JsonObject::new();
    meta.insert(RESULT_STATUS_META_KEY.to_string(), status_payload);

    let content = vec![rmcp::model::Content::text(text)];
    let result = if status.outcome_class().is_error() {
        rmcp::model::CallToolResult::error(content)
    } else {
        rmcp::model::CallToolResult::success(content)
    };
    Ok(result.with_meta(Some(rmcp::model::Meta(meta))))
}

fn is_index_unavailable_output(text: &str) -> bool {
    text.starts_with("Index not loaded.")
        || text.starts_with("Index is loading")
        || text.starts_with("Index degraded:")
}

fn is_error_output(text: &str) -> bool {
    text.starts_with("Error:") || text.starts_with("Error in ")
}

fn classify_edit_output(text: &str, dry_run: bool) -> EditResultStatus {
    if is_index_unavailable_output(text) {
        EditResultStatus::InternalFailure
    } else if text.contains("Ambiguous:") || text.starts_with("Ambiguous:") {
        EditResultStatus::Ambiguous
    } else if text.starts_with("File not found:")
        || text.starts_with("Symbol not found:")
        || text.contains("Symbol not found:")
        || text.contains("File not indexed:")
    {
        EditResultStatus::NotFound
    } else if text.contains("no repository root configured")
        || text.contains("still loading")
        || text.contains("unavailable")
        || text.starts_with("Error writing ")
        || text.contains("Write failed")
        || text.contains("ROLLBACK INCOMPLETE")
        || text.contains("File disappeared:")
        || text.contains("byte range")
        || text.contains("Session stale")
    {
        EditResultStatus::InternalFailure
    } else if is_error_output(text)
        || text.starts_with("ProjectConfigTrustEnforced:")
        || text.starts_with("Overlapping edits")
        || text.contains("path escapes repo root")
        || text.contains("Path containment error")
        || text.contains("Path resolution error")
    {
        EditResultStatus::InvalidRequest
    } else if dry_run
        || text.contains("[DRY RUN]")
        || text.contains("Write semantics: dry run (no writes)")
    {
        EditResultStatus::DryRunSuccess
    } else {
        EditResultStatus::Success
    }
}

fn success_operation_statuses(
    count: usize,
    status: EditResultStatus,
) -> Vec<(usize, EditResultStatus)> {
    (1..=count).map(|index| (index, status)).collect()
}

fn failed_batch_operation_statuses(
    text: &str,
    status: EditResultStatus,
) -> Vec<(usize, EditResultStatus)> {
    let Some(rest) = text.strip_prefix("Edit ") else {
        return Vec::new();
    };
    let digits = rest
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    let Some(':') = rest.chars().nth(digits.len()) else {
        return Vec::new();
    };
    match digits.parse::<usize>() {
        Ok(index) => vec![(index, status)],
        Err(_) => Vec::new(),
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum EditError {
    #[error("Error: file not found at {path}")]
    PathNotFound { path: std::path::PathBuf },
    #[error("Error: session stale at {path} — {recovery}")]
    SessionStale {
        path: std::path::PathBuf,
        recovery: String,
    },
}

/// Surface-aware recovery clause for a stale edit session (TR-02 / FR-012).
///
/// "Refresh repo_root" maps to `index_folder` on the full surface, but that tool
/// is dispatch-gated on the compact surface (`symforge_edit` is reachable there,
/// `index_folder` is not). Computed from the active surface so the message never
/// names a forbidden capability.
fn session_stale_recovery() -> String {
    match crate::protocol::surface_probe::surface_profile_from_env() {
        crate::protocol::surface_probe::SurfaceProfile::Compact => {
            "set SYMFORGE_WORKSPACE_ROOT to the project path (or run `symforge init` for \
             this harness) and reconnect to refresh repo_root, or set SYMFORGE_SURFACE=full \
             for the full tool surface"
                .to_string()
        }
        crate::protocol::surface_probe::SurfaceProfile::Full
        | crate::protocol::surface_probe::SurfaceProfile::Meta => {
            "call index_folder to refresh repo_root".to_string()
        }
    }
}

fn edit_capability_label(
    capability: crate::parsing::config_extractors::EditCapability,
) -> &'static str {
    use crate::parsing::config_extractors::EditCapability;

    match capability {
        EditCapability::IndexOnly => "index-only",
        EditCapability::TextEditSafe => "text-edit-safe",
        EditCapability::StructuralEditSafe => "structural-edit-safe",
    }
}

pub(crate) fn prepare_exact_path_for_edit(
    server: &SymForgeServer,
    relative_path: &str,
) -> Result<(PathBuf, edit_format::EditSourceAuthority), String> {
    let expected_gen = server.index.current_project_generation();
    let repo_root = server
        .capture_repo_root()
        .ok_or_else(|| "Error: no repository root configured.".to_string())?;
    let abs_path =
        safe_repo_path_for_freshen(&repo_root, relative_path).map_err(|e| format!("Error: {e}"))?;
    let source_authority =
        match watcher::freshen_file_if_stale(relative_path, &abs_path, &server.index, expected_gen)
        {
            watcher::FreshenResult::Fresh => edit_format::EditSourceAuthority::CurrentIndex,
            watcher::FreshenResult::StaleReindexed => {
                edit_format::EditSourceAuthority::DiskRefreshed
            }
            watcher::FreshenResult::StaleRemoved => {
                return Err(format!("{}", EditError::PathNotFound { path: abs_path }));
            }
            watcher::FreshenResult::GenerationMismatch => {
                return Err(format!(
                    "{}",
                    EditError::SessionStale {
                        path: abs_path,
                        recovery: session_stale_recovery(),
                    }
                ));
            }
        };
    Ok((abs_path, source_authority))
}

pub(super) fn prepare_batch_paths_for_edit(
    server: &SymForgeServer,
    relative_paths: &[String],
) -> Result<(PathBuf, edit_format::EditSourceAuthority), String> {
    let expected_gen = server.index.current_project_generation();
    let repo_root = server
        .capture_repo_root()
        .ok_or_else(|| "Error: no repository root configured.".to_string())?;
    let mut unique_paths = relative_paths.to_vec();
    unique_paths.sort();
    unique_paths.dedup();

    let mut refreshed = false;
    for relative_path in unique_paths {
        let abs_path = safe_repo_path_for_freshen(&repo_root, &relative_path)
            .map_err(|e| format!("Error: {e}"))?;
        match watcher::freshen_file_if_stale(&relative_path, &abs_path, &server.index, expected_gen)
        {
            watcher::FreshenResult::Fresh => {}
            watcher::FreshenResult::StaleReindexed => {
                refreshed = true;
            }
            watcher::FreshenResult::StaleRemoved => {
                tracing::warn!(
                    path = %abs_path.display(),
                    "skipping missing path during batch edit preparation"
                );
            }
            watcher::FreshenResult::GenerationMismatch => {
                return Err(format!(
                    "{}",
                    EditError::SessionStale {
                        path: abs_path,
                        recovery: session_stale_recovery(),
                    }
                ));
            }
        }
    }

    let source_authority = if refreshed {
        edit_format::EditSourceAuthority::DiskRefreshed
    } else {
        edit_format::EditSourceAuthority::CurrentIndex
    };
    Ok((repo_root, source_authority))
}

fn prepare_project_wide_rename(
    server: &SymForgeServer,
    repo_root: &std::path::Path,
) -> edit_format::EditSourceAuthority {
    if watcher::reconcile_stale_files(repo_root, &server.index) > 0 {
        edit_format::EditSourceAuthority::DiskRefreshed
    } else {
        edit_format::EditSourceAuthority::CurrentIndex
    }
}

fn begin_mutation_replay<T: Serialize>(
    repo_root: &Path,
    tool_name: &str,
    input: &T,
    idempotency_key: Option<&str>,
    dry_run: bool,
) -> Result<Option<crate::idempotency::ActiveReplay>, String> {
    if dry_run {
        return Ok(None);
    }
    let Some(raw_key) = idempotency_key else {
        return Ok(None);
    };

    let mut request = serde_json::to_value(input)
        .map_err(|error| crate::idempotency::format_tool_error(&error.into()))?;
    if let serde_json::Value::Object(map) = &mut request {
        map.remove("idempotency_key");
    }

    match crate::idempotency::begin_tool_replay(repo_root, tool_name, raw_key, &request) {
        Ok(crate::idempotency::ReplayStart::FirstExecution(active)) => Ok(Some(active)),
        Ok(crate::idempotency::ReplayStart::Replay(response)) => Err(response),
        Err(error) => Err(crate::idempotency::format_tool_error(&error)),
    }
}

fn complete_mutation_replay(
    idempotency: &Option<crate::idempotency::ActiveReplay>,
    output: &mut String,
) {
    if let Some(idempotency) = idempotency
        && let Err(error) = idempotency.complete(output.clone())
    {
        output.push_str(&format!(
            "\nIdempotency warning: failed to store replay result: {error}"
        ));
    }
}

fn fail_mutation_replay(idempotency: &Option<crate::idempotency::ActiveReplay>, output: &str) {
    if let Some(idempotency) = idempotency {
        let _ = idempotency.fail(output.to_string());
    }
}

fn fail_and_return_mutation_replay(
    idempotency: &Option<crate::idempotency::ActiveReplay>,
    output: String,
) -> String {
    fail_mutation_replay(idempotency, &output);
    output
}

fn symbol_anchor(path: &str, symbol: &crate::domain::SymbolRecord) -> String {
    format!("{path}:{}", symbol.line_range.0.saturating_add(1))
}

#[tool_router(router = edit_tool_router, vis = "pub(crate)")]
impl SymForgeServer {
    // ─── Edit tools (Tier 1) ─────────────────────────────────────────────────

    pub(super) fn check_edit_capability(
        language: &crate::domain::LanguageId,
        required: crate::parsing::config_extractors::EditCapability,
        tool_name: &str,
    ) -> Option<String> {
        use crate::parsing::config_extractors::{EditCapability, edit_capability_for_language};
        if let Some(cap) = edit_capability_for_language(language) {
            let allowed = match required {
                EditCapability::IndexOnly => false,
                EditCapability::TextEditSafe => {
                    matches!(
                        cap,
                        EditCapability::TextEditSafe | EditCapability::StructuralEditSafe
                    )
                }
                EditCapability::StructuralEditSafe => {
                    matches!(cap, EditCapability::StructuralEditSafe)
                }
            };
            if !allowed {
                let suggestion = match required {
                    EditCapability::StructuralEditSafe => {
                        "use edit_within_symbol for scoped text replacements, or the built-in Edit tool for raw text edits."
                    }
                    EditCapability::TextEditSafe => {
                        "use the built-in Edit tool for raw text edits in this file type."
                    }
                    EditCapability::IndexOnly => {
                        "inspect the file with read-only tools or use the built-in Edit tool for raw text edits."
                    }
                };
                return Some(edit_format::format_capability_warning(
                    tool_name,
                    &language.to_string(),
                    edit_capability_label(required),
                    edit_capability_label(cap),
                    suggestion,
                ));
            }
        }
        None // No capability restriction
    }

    /// Append the success-only post-edit impact footer for `path` to `output`.
    ///
    /// Mirrors `append_project_config_trust_suffix`: a single leading newline then
    /// the footer text. Computes the distinct dependent file count and (when git
    /// temporal data is `Ready`) the top co-change partners via
    /// `format::edit_impact_summary`. The dependents come from the read snapshot
    /// (`self.index.read()` → `&LiveIndex`) and the co-changes from the lock-free
    /// temporal snapshot on the shared handle (`self.index.git_temporal()`). If the
    /// index is not `Ready` (loading/empty), nothing is appended — the footer is
    /// best-effort and never blocks a successful edit response.
    fn append_impact_footer(&self, output: &mut String, path: &str) {
        let guard = self.index.read();
        if !matches!(guard.index_state(), IndexState::Ready) {
            return;
        }
        let temporal = self.index.git_temporal();
        let (deps, cochanges) = format::edit_impact_summary(&guard, &temporal, path);
        output.push('\n');
        output.push_str(&format::impact_footer(deps, &cochanges));
    }

    /// Replace a symbol's entire definition with new source code. The index resolves the symbol's
    /// byte range server-side — no need to read the file first. Content is auto-indented to match
    /// the original symbol's indentation level.
    /// NOT for small edits within a symbol (use edit_within_symbol).
    /// NOT for removing a symbol entirely (use delete_symbol).
    #[tool(
        name = "replace_symbol_body",
        description = "Replace a symbol's entire definition with new source code. The index resolves the symbol's byte range server-side — no need to read the file first. Content is auto-indented to match the original symbol's indentation level. Use symbol_line to disambiguate overloaded names. NOT for small edits within a symbol (use edit_within_symbol). NOT for removing a symbol entirely (use delete_symbol).",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    pub(crate) async fn replace_symbol_body_tool(
        &self,
        params: Parameters<edit::ReplaceSymbolBodyInput>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        let started = Instant::now();
        let dry_run = params.0.dry_run.unwrap_or(false);
        let output = self.replace_symbol_body(params).await;
        let status = classify_edit_output(&output, dry_run);
        self.record_tool_completion(
            "replace_symbol_body",
            &output,
            started.elapsed(),
            status.outcome_class(),
        );
        statused_edit_tool_result(output, status, Vec::new())
    }

    pub(crate) async fn replace_symbol_body(
        &self,
        params: Parameters<edit::ReplaceSymbolBodyInput>,
    ) -> String {
        if let Some(result) = self.proxy_tool_call("replace_symbol_body", &params.0).await {
            return result;
        }
        self.note_worktree_misuse_if_active(params.0.working_directory.as_deref());
        {
            let guard = self.index.read();
            loading_guard!(guard);
            if guard.capture_shared_file(&params.0.path).is_none() {
                return format::not_found_file(&params.0.path);
            }
        }
        let (abs_path, source_authority) = match prepare_exact_path_for_edit(self, &params.0.path) {
            Ok(prepared) => prepared,
            Err(e) => return e,
        };
        let repo_root = match self.capture_repo_root() {
            Some(root) => root,
            None => return "Error: no repository root configured.".to_string(),
        };
        let project_config_trust_suffix = match project_config_trust_response_suffix(&repo_root) {
            Ok(suffix) => suffix,
            Err(message) => return message,
        };
        let idempotency = match begin_mutation_replay(
            &repo_root,
            "replace_symbol_body",
            &params.0,
            params.0.idempotency_key.as_deref(),
            params.0.dry_run.unwrap_or(false),
        ) {
            Ok(idempotency) => idempotency,
            Err(output) => return output,
        };
        let working_directory = params
            .0
            .working_directory
            .as_deref()
            .map(std::path::Path::new);
        let hook_ctx = edit_hooks::EditContext {
            relative_path: &params.0.path,
            indexed_absolute_path: &abs_path,
            repo_root: &repo_root,
            working_directory,
        };
        let resolved_target = match edit_hooks::resolve(&hook_ctx) {
            Ok(r) => r,
            Err(e) => return fail_and_return_mutation_replay(&idempotency, format!("Error: {e}")),
        };
        let resolved_path = resolved_target.target_path.clone();
        let file = {
            let guard = self.index.read();
            loading_guard!(guard);
            guard.capture_shared_file(&params.0.path)
        };
        let file = match file {
            Some(f) => f,
            None => {
                return fail_and_return_mutation_replay(
                    &idempotency,
                    format::not_found_file(&params.0.path),
                );
            }
        };
        // Review finding 5 (post-v7.19.0): a rerouted edit must splice into
        // the worktree TARGET's current bytes. The index mirrors the indexed
        // copy (routed writes never touch it), so using index content as the
        // base would silently discard every earlier routed edit to this file.
        let edit_base = match edit::rebase_edit_base_for_reroute(file, &resolved_target) {
            Ok(base) => base,
            Err(e) => return fail_and_return_mutation_replay(&idempotency, e),
        };
        let source_authority = if edit_base.rebased {
            edit_format::EditSourceAuthority::WorktreeTarget
        } else {
            source_authority
        };
        let file = edit_base.file;
        if let Some(warning) = Self::check_edit_capability(
            &file.language,
            crate::parsing::config_extractors::EditCapability::StructuralEditSafe,
            "replace_symbol_body",
        ) {
            return fail_and_return_mutation_replay(&idempotency, warning);
        }
        let (_, sym) = match edit::resolve_or_error(
            &file,
            &params.0.name,
            params.0.kind.as_deref(),
            params.0.symbol_line,
        ) {
            Ok(s) => s,
            Err(e) => return fail_and_return_mutation_replay(&idempotency, e),
        };
        let evidence_anchor = symbol_anchor(&params.0.path, &sym);
        if params.0.dry_run == Some(true) {
            let old_bytes = (sym.byte_range.1 - sym.byte_range.0) as usize;
            let summary = format!(
                "[DRY RUN] Would replace `{}` in {} (old: {} bytes -> new: {} bytes)",
                params.0.name,
                params.0.path,
                old_bytes,
                params.0.new_body.len()
            );
            let mut result = format!(
                "{}\n{}",
                edit_format::format_edit_envelope(
                    edit_format::EditSafetyMode::StructuralEditSafe,
                    source_authority,
                    edit_format::EditWriteSemantics::DryRunNoWrites,
                    &evidence_anchor,
                ),
                summary
            );
            append_project_config_trust_suffix(&mut result, project_config_trust_suffix.as_deref());
            return result;
        }
        let old_bytes = (sym.byte_range.1 - sym.byte_range.0) as usize;
        // Decide where the splice starts based on whether the caller
        // supplied fresh docs in `new_body`:
        //   * new_body starts with a doc marker → extend past the old
        //     attached/orphaned docs so the new ones replace them
        //     (prevents duplicate JSDoc/XML doc blocks).
        //   * new_body has no doc marker → preserve existing attached docs
        //     and attributes. If an inline doc marker shares the symbol line,
        //     start just after that marker so the old modifier/signature is
        //     still replaced by the caller's body.
        // Preserving docs by default was the behavior users expected;
        // swallowing them silently was the bug surfaced in the v7.5 review.
        let new_body_supplies_docs = edit::body_starts_with_doc_comment(&params.0.new_body);
        let effective = if new_body_supplies_docs {
            sym.effective_start() as usize
        } else {
            sym.byte_range.0 as usize
        };
        let raw_line_start = file.content[..effective]
            .iter()
            .rposition(|&b| b == b'\n')
            .map(|p| p + 1)
            .unwrap_or(0);
        let line_start = if new_body_supplies_docs {
            edit::extend_past_orphaned_docs(&file.content, raw_line_start, &sym) as u32
        } else {
            edit::docless_replacement_splice_start(
                &file.content,
                raw_line_start,
                sym.byte_range.0 as usize,
            ) as u32
        };
        let indent = edit::detect_indentation(&file.content, sym.byte_range.0);
        let line_ending = edit::detect_line_ending(&file.content);
        let normalized = edit::normalize_line_endings(params.0.new_body.as_bytes(), line_ending);
        let normalized_str = std::str::from_utf8(&normalized).unwrap_or(&params.0.new_body);
        let indented = edit::apply_indentation(normalized_str, &indent, line_ending);
        let new_content =
            edit::apply_splice(&file.content, (line_start, sym.byte_range.1), &indented);
        // TR-06 / FR-009 (design D1): re-verify the `if_match` guard against the
        // bytes ACTUALLY on disk, in the same per-path-locked critical section
        // as the write. `file.content` is the exact base the splice in
        // `new_content` was computed against (the index snapshot for a
        // pass-through edit, or the rebased worktree target). If the caller
        // supplied `if_match` and the on-disk bytes diverged from that base
        // after their read, a concurrent writer changed the file: reject without
        // writing so the concurrent change is preserved — and report a failed
        // guarded apply, never a success (FR-010). The kept STEL pre-flight
        // (`run_pre_apply_gates`) still fails fast; THIS is the actual guarantee
        // at the write.
        //
        // EXACT-BYTE comparison (`base == disk`) is correct here ONLY because
        // the compact `if_match` path never reroutes: `StelEditRequest` carries
        // no `working_directory`, so `resolved_path` is always the indexed file
        // and `file.content` is its exact (line-ending-preserving) bytes. If
        // `if_match` is ever plumbed through a worktree reroute, the base would
        // be a rebased target whose line endings may differ, and this guard MUST
        // then reconcile with `edit::line_ending_insensitive_eq` like
        // `guard_batch_reroute_divergence` does — an exact-byte compare would
        // spuriously reject on a pure CRLF/LF difference.
        let write_report = match edit::guarded_atomic_write_file(
            &resolved_path,
            &file.content,
            &new_content,
            params.0.if_match.as_deref(),
        ) {
            Ok(edit::GuardedWriteOutcome::Written(report)) => report,
            Ok(edit::GuardedWriteOutcome::Rejected) => {
                let output = format!(
                    "{} — guarded apply rejected: on-disk body of `{}` in {} \
                     changed since if_match was captured; the concurrent change was left intact \
                     and nothing was written. Re-read the symbol and retry.",
                    edit_format::WRITE_MODE_FAILED_SENTINEL,
                    params.0.name,
                    params.0.path
                );
                fail_mutation_replay(&idempotency, &output);
                return output;
            }
            Err(e) => {
                let output = format!("Error writing {}: {e}", params.0.path);
                fail_mutation_replay(&idempotency, &output);
                return output;
            }
        };
        let old_sig = edit::extract_signature(&file.content, sym.byte_range);
        let new_sig = params.0.new_body.lines().next().unwrap_or("").to_string();
        // Detect parent impl type for type-aware reference filtering.
        // Methods inside `impl Foo` only warn about refs in files that also mention `Foo`.
        let parent_type = edit::find_parent_impl_type(&file, &sym);
        // Review finding 5 (post-v7.19.0): only a pass-through write updates
        // the index. A routed write leaves the indexed copy untouched, so
        // replacing the index entry with worktree bytes would make the index
        // lie about the indexed root — and the next edit's freshness check
        // would "correct" it back from disk, erasing the routed state it was
        // spliced from.
        if !resolved_target.rerouted {
            edit::reindex_after_write(
                &self.index,
                &resolved_path,
                &params.0.path,
                &new_content,
                file.language.clone(),
            );
            // DORMANT SEAM (012 US3/SC-003/FR-005). This populates the per-session
            // overlay, but NO production read path consumes the delta today: the
            // get_symbol overlay read was removed (redundant), and cross-project
            // refresh_working_set_bases re-interns the base from the live index and
            // attaches a fresh EMPTY overlay. The base is already updated by
            // reindex_after_write above, so every read sees the edit via the base.
            // The writer is kept as a deliberate seam for FUTURE commit-gated
            // session-private edits (precondition #1 in
            // docs/reviews/overlay-redundancy-decision.md): until edits stop writing
            // through to the shared base, the overlay can only ever duplicate it.
            // `None` on the shared instance and in local-stdio mode — byte-identical.
            // The overlay lock is taken AFTER reindex_after_write returned (no
            // index-lock nesting, I2) and while holding NO daemon-map lock (I1).
            if let Some(ov) = &self.session_working_set
                && let Some(parsed) = edit::parse_indexed_for_overlay(
                    &resolved_path,
                    &params.0.path,
                    file.language.clone(),
                )
            {
                let mut ws = ov.working_set.write();
                if let Some(entry) = ws.get_mut(&ov.project_id) {
                    entry.overlay.upsert(params.0.path.clone(), parsed);
                }
                // entry `None`: the active project was not seeded into this
                // session's working set (a wiring bug — see spec stop_conditions).
                // Best-effort: the base was already updated by reindex_after_write,
                // so reads still see the edit via the base fall-through.
            }
        }
        edit_hooks::after_commit(&hook_ctx, &resolved_path);
        let warnings = edit::detect_stale_references(
            &self.index,
            &params.0.path,
            &params.0.name,
            &old_sig,
            &new_sig,
            parent_type.as_deref(),
            Some(&file.language),
        );
        let mut result = format!(
            "{}\n{}",
            edit_format::format_edit_envelope(
                edit_format::EditSafetyMode::StructuralEditSafe,
                source_authority,
                edit_format::EditWriteSemantics::AtomicWriteAndReindex,
                &evidence_anchor,
            ),
            edit_format::format_replace(
                &params.0.path,
                &params.0.name,
                &sym.kind.to_string(),
                old_bytes,
                indented.len(),
            )
        );
        result.push_str(&edit_format::format_stale_warnings(
            &params.0.path,
            &params.0.name,
            &warnings,
        ));
        result.push_str(&edit::format_tee_snapshot_suffix(&write_report));
        result.push_str(&edit_format::format_reroute_suffix(
            working_directory,
            &resolved_target,
        ));
        append_project_config_trust_suffix(&mut result, project_config_trust_suffix.as_deref());
        self.append_impact_footer(&mut result, &params.0.path);
        complete_mutation_replay(&idempotency, &mut result);
        result
    }

    /// Insert code before or after a named symbol. Content is auto-indented to match the target
    /// symbol's indentation level — provide unindented code.
    /// NOT for replacing existing code (use replace_symbol_body or edit_within_symbol).
    #[tool(
        description = "Insert code before or after a named symbol. Set position='before' or 'after' (default 'after'). Content is auto-indented to match the target symbol's indentation level — provide unindented code. Use symbol_line to disambiguate overloaded names. NOT for replacing existing code (use replace_symbol_body or edit_within_symbol).",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    pub(crate) async fn insert_symbol(
        &self,
        params: Parameters<edit::InsertSymbolInput>,
    ) -> String {
        if let Some(result) = self.proxy_tool_call("insert_symbol", &params.0).await {
            return result;
        }
        self.note_worktree_misuse_if_active(params.0.working_directory.as_deref());
        let position = params.0.position.as_deref().unwrap_or("after");
        if position != "before" && position != "after" {
            return format!("Error: position must be 'before' or 'after', got '{position}'");
        }
        {
            let guard = self.index.read();
            loading_guard!(guard);
            if guard.capture_shared_file(&params.0.path).is_none() {
                return format::not_found_file(&params.0.path);
            }
        }
        let (abs_path, source_authority) = match prepare_exact_path_for_edit(self, &params.0.path) {
            Ok(prepared) => prepared,
            Err(e) => return e,
        };
        let repo_root = match self.capture_repo_root() {
            Some(root) => root,
            None => return "Error: no repository root configured.".to_string(),
        };
        let project_config_trust_suffix = match project_config_trust_response_suffix(&repo_root) {
            Ok(suffix) => suffix,
            Err(message) => return message,
        };
        let idempotency = match begin_mutation_replay(
            &repo_root,
            "insert_symbol",
            &params.0,
            params.0.idempotency_key.as_deref(),
            params.0.dry_run.unwrap_or(false),
        ) {
            Ok(idempotency) => idempotency,
            Err(output) => return output,
        };
        let working_directory = params
            .0
            .working_directory
            .as_deref()
            .map(std::path::Path::new);
        let hook_ctx = edit_hooks::EditContext {
            relative_path: &params.0.path,
            indexed_absolute_path: &abs_path,
            repo_root: &repo_root,
            working_directory,
        };
        let resolved_target = match edit_hooks::resolve(&hook_ctx) {
            Ok(r) => r,
            Err(e) => return fail_and_return_mutation_replay(&idempotency, format!("Error: {e}")),
        };
        let resolved_path = resolved_target.target_path.clone();
        let file = {
            let guard = self.index.read();
            loading_guard!(guard);
            guard.capture_shared_file(&params.0.path)
        };
        let file = match file {
            Some(f) => f,
            None => {
                return fail_and_return_mutation_replay(
                    &idempotency,
                    format::not_found_file(&params.0.path),
                );
            }
        };
        // Review finding 5 (post-v7.19.0): a rerouted edit must splice into
        // the worktree TARGET's current bytes. The index mirrors the indexed
        // copy (routed writes never touch it), so using index content as the
        // base would silently discard every earlier routed edit to this file.
        let edit_base = match edit::rebase_edit_base_for_reroute(file, &resolved_target) {
            Ok(base) => base,
            Err(e) => return fail_and_return_mutation_replay(&idempotency, e),
        };
        let source_authority = if edit_base.rebased {
            edit_format::EditSourceAuthority::WorktreeTarget
        } else {
            source_authority
        };
        let file = edit_base.file;
        if let Some(warning) = Self::check_edit_capability(
            &file.language,
            crate::parsing::config_extractors::EditCapability::StructuralEditSafe,
            "insert_symbol",
        ) {
            return fail_and_return_mutation_replay(&idempotency, warning);
        }
        let (_, sym) = match edit::resolve_or_error(
            &file,
            &params.0.name,
            params.0.kind.as_deref(),
            params.0.symbol_line,
        ) {
            Ok(s) => s,
            Err(e) => return fail_and_return_mutation_replay(&idempotency, e),
        };
        let evidence_anchor = symbol_anchor(&params.0.path, &sym);
        if params.0.dry_run == Some(true) {
            let summary = format!(
                "[DRY RUN] Would insert {} `{}` in {} ({} bytes of content)",
                position,
                params.0.name,
                params.0.path,
                params.0.content.len()
            );
            let mut result = format!(
                "{}\n{}",
                edit_format::format_edit_envelope(
                    edit_format::EditSafetyMode::StructuralEditSafe,
                    source_authority,
                    edit_format::EditWriteSemantics::DryRunNoWrites,
                    &evidence_anchor,
                ),
                summary
            );
            append_project_config_trust_suffix(&mut result, project_config_trust_suffix.as_deref());
            return result;
        }
        let line_ending = edit::detect_line_ending(&file.content);
        let new_content = if position == "before" {
            edit::build_insert_before(&file.content, &sym, &params.0.content, line_ending)
        } else {
            edit::build_insert_after(&file.content, &sym, &params.0.content, line_ending)
        };
        let write_report = match edit::atomic_write_file(&resolved_path, &new_content) {
            Ok(report) => report,
            Err(e) => {
                let output = format!("Error writing {}: {e}", params.0.path);
                fail_mutation_replay(&idempotency, &output);
                return output;
            }
        };
        // Review finding 5 (post-v7.19.0): only a pass-through write updates
        // the index. A routed write leaves the indexed copy untouched, so
        // replacing the index entry with worktree bytes would make the index
        // lie about the indexed root — and the next edit's freshness check
        // would "correct" it back from disk, erasing the routed state it was
        // spliced from.
        if !resolved_target.rerouted {
            edit::reindex_after_write(
                &self.index,
                &resolved_path,
                &params.0.path,
                &new_content,
                file.language.clone(),
            );
        }
        edit_hooks::after_commit(&hook_ctx, &resolved_path);
        let mut out = format!(
            "{}\n{}",
            edit_format::format_edit_envelope(
                edit_format::EditSafetyMode::StructuralEditSafe,
                source_authority,
                edit_format::EditWriteSemantics::AtomicWriteAndReindex,
                &evidence_anchor,
            ),
            edit_format::format_insert(
                &params.0.path,
                &params.0.name,
                position,
                params.0.content.len(),
            )
        );
        out.push_str(&edit_format::format_reroute_suffix(
            working_directory,
            &resolved_target,
        ));
        out.push_str(&edit::format_tee_snapshot_suffix(&write_report));
        append_project_config_trust_suffix(&mut out, project_config_trust_suffix.as_deref());
        self.append_impact_footer(&mut out, &params.0.path);
        complete_mutation_replay(&idempotency, &mut out);
        out
    }

    /// Remove a symbol's entire definition and clean up surrounding blank lines.
    /// NOT for replacing a symbol (use replace_symbol_body).
    #[tool(
        description = "Remove a symbol's entire definition and clean up surrounding blank lines. Use symbol_line to disambiguate overloaded names. NOT for replacing a symbol (use replace_symbol_body).",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    pub(crate) async fn delete_symbol(
        &self,
        params: Parameters<edit::DeleteSymbolInput>,
    ) -> String {
        if let Some(result) = self.proxy_tool_call("delete_symbol", &params.0).await {
            return result;
        }
        self.note_worktree_misuse_if_active(params.0.working_directory.as_deref());
        {
            let guard = self.index.read();
            loading_guard!(guard);
            if guard.capture_shared_file(&params.0.path).is_none() {
                return format::not_found_file(&params.0.path);
            }
        }
        let (abs_path, source_authority) = match prepare_exact_path_for_edit(self, &params.0.path) {
            Ok(prepared) => prepared,
            Err(e) => return e,
        };
        let repo_root = match self.capture_repo_root() {
            Some(root) => root,
            None => return "Error: no repository root configured.".to_string(),
        };
        let project_config_trust_suffix = match project_config_trust_response_suffix(&repo_root) {
            Ok(suffix) => suffix,
            Err(message) => return message,
        };
        let idempotency = match begin_mutation_replay(
            &repo_root,
            "delete_symbol",
            &params.0,
            params.0.idempotency_key.as_deref(),
            params.0.dry_run.unwrap_or(false),
        ) {
            Ok(idempotency) => idempotency,
            Err(output) => return output,
        };
        let working_directory = params
            .0
            .working_directory
            .as_deref()
            .map(std::path::Path::new);
        let hook_ctx = edit_hooks::EditContext {
            relative_path: &params.0.path,
            indexed_absolute_path: &abs_path,
            repo_root: &repo_root,
            working_directory,
        };
        let resolved_target = match edit_hooks::resolve(&hook_ctx) {
            Ok(r) => r,
            Err(e) => return fail_and_return_mutation_replay(&idempotency, format!("Error: {e}")),
        };
        let resolved_path = resolved_target.target_path.clone();
        let file = {
            let guard = self.index.read();
            loading_guard!(guard);
            guard.capture_shared_file(&params.0.path)
        };
        let file = match file {
            Some(f) => f,
            None => {
                return fail_and_return_mutation_replay(
                    &idempotency,
                    format::not_found_file(&params.0.path),
                );
            }
        };
        // Review finding 5 (post-v7.19.0): a rerouted edit must splice into
        // the worktree TARGET's current bytes. The index mirrors the indexed
        // copy (routed writes never touch it), so using index content as the
        // base would silently discard every earlier routed edit to this file.
        let edit_base = match edit::rebase_edit_base_for_reroute(file, &resolved_target) {
            Ok(base) => base,
            Err(e) => return fail_and_return_mutation_replay(&idempotency, e),
        };
        let source_authority = if edit_base.rebased {
            edit_format::EditSourceAuthority::WorktreeTarget
        } else {
            source_authority
        };
        let file = edit_base.file;
        if let Some(warning) = Self::check_edit_capability(
            &file.language,
            crate::parsing::config_extractors::EditCapability::StructuralEditSafe,
            "delete_symbol",
        ) {
            return fail_and_return_mutation_replay(&idempotency, warning);
        }
        let (_, sym) = match edit::resolve_or_error(
            &file,
            &params.0.name,
            params.0.kind.as_deref(),
            params.0.symbol_line,
        ) {
            Ok(s) => s,
            Err(e) => return fail_and_return_mutation_replay(&idempotency, e),
        };
        let evidence_anchor = symbol_anchor(&params.0.path, &sym);
        if params.0.dry_run == Some(true) {
            let deleted_bytes = (sym.byte_range.1 - sym.byte_range.0) as usize;
            let summary = format!(
                "[DRY RUN] Would delete `{}` in {} ({} bytes)",
                params.0.name, params.0.path, deleted_bytes
            );
            let mut result = format!(
                "{}\n{}",
                edit_format::format_edit_envelope(
                    edit_format::EditSafetyMode::StructuralEditSafe,
                    source_authority,
                    edit_format::EditWriteSemantics::DryRunNoWrites,
                    &evidence_anchor,
                ),
                summary
            );
            append_project_config_trust_suffix(&mut result, project_config_trust_suffix.as_deref());
            return result;
        }
        let deleted_bytes = (sym.byte_range.1 - sym.byte_range.0) as usize;
        let line_ending = edit::detect_line_ending(&file.content);
        let new_content = edit::build_delete(&file.content, &sym, line_ending);
        let write_report = match edit::atomic_write_file(&resolved_path, &new_content) {
            Ok(report) => report,
            Err(e) => {
                let output = format!("Error writing {}: {e}", params.0.path);
                fail_mutation_replay(&idempotency, &output);
                return output;
            }
        };
        // Review finding 5 (post-v7.19.0): only a pass-through write updates
        // the index. A routed write leaves the indexed copy untouched, so
        // replacing the index entry with worktree bytes would make the index
        // lie about the indexed root — and the next edit's freshness check
        // would "correct" it back from disk, erasing the routed state it was
        // spliced from.
        if !resolved_target.rerouted {
            edit::reindex_after_write(
                &self.index,
                &resolved_path,
                &params.0.path,
                &new_content,
                file.language.clone(),
            );
        }
        edit_hooks::after_commit(&hook_ctx, &resolved_path);
        let mut out = format!(
            "{}\n{}",
            edit_format::format_edit_envelope(
                edit_format::EditSafetyMode::StructuralEditSafe,
                source_authority,
                edit_format::EditWriteSemantics::AtomicWriteAndReindex,
                &evidence_anchor,
            ),
            edit_format::format_delete(
                &params.0.path,
                &params.0.name,
                &sym.kind.to_string(),
                deleted_bytes,
            )
        );
        out.push_str(&edit_format::format_reroute_suffix(
            working_directory,
            &resolved_target,
        ));
        out.push_str(&edit::format_tee_snapshot_suffix(&write_report));
        append_project_config_trust_suffix(&mut out, project_config_trust_suffix.as_deref());
        self.append_impact_footer(&mut out, &params.0.path);
        complete_mutation_replay(&idempotency, &mut out);
        out
    }

    /// Find-and-replace scoped to a symbol's byte range — won't affect code outside it. The LLM
    /// never needs to read the symbol body — just provide the old and new text.
    /// NOT for replacing the entire symbol (use replace_symbol_body).
    /// NOT for adding new symbols (use insert_before/after_symbol).
    #[tool(
        description = "Find-and-replace scoped to a symbol's byte range — won't affect code outside it. The LLM never needs to read the symbol body — just provide the old and new text. Set replace_all=true for every occurrence within the symbol. NOT for replacing the entire symbol (use replace_symbol_body). NOT for adding new symbols (use insert_before/after_symbol).",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    pub(crate) async fn edit_within_symbol(
        &self,
        params: Parameters<edit::EditWithinSymbolInput>,
    ) -> String {
        if let Some(result) = self.proxy_tool_call("edit_within_symbol", &params.0).await {
            return result;
        }
        self.note_worktree_misuse_if_active(params.0.working_directory.as_deref());
        {
            let guard = self.index.read();
            loading_guard!(guard);
            if guard.capture_shared_file(&params.0.path).is_none() {
                return format::not_found_file(&params.0.path);
            }
        }
        let (abs_path, source_authority) = match prepare_exact_path_for_edit(self, &params.0.path) {
            Ok(prepared) => prepared,
            Err(e) => return e,
        };
        let repo_root = match self.capture_repo_root() {
            Some(root) => root,
            None => return "Error: no repository root configured.".to_string(),
        };
        let project_config_trust_suffix = match project_config_trust_response_suffix(&repo_root) {
            Ok(suffix) => suffix,
            Err(message) => return message,
        };
        let idempotency = match begin_mutation_replay(
            &repo_root,
            "edit_within_symbol",
            &params.0,
            params.0.idempotency_key.as_deref(),
            params.0.dry_run.unwrap_or(false),
        ) {
            Ok(idempotency) => idempotency,
            Err(output) => return output,
        };
        let working_directory = params
            .0
            .working_directory
            .as_deref()
            .map(std::path::Path::new);
        let hook_ctx = edit_hooks::EditContext {
            relative_path: &params.0.path,
            indexed_absolute_path: &abs_path,
            repo_root: &repo_root,
            working_directory,
        };
        let resolved_target = match edit_hooks::resolve(&hook_ctx) {
            Ok(r) => r,
            Err(e) => return fail_and_return_mutation_replay(&idempotency, format!("Error: {e}")),
        };
        let resolved_path = resolved_target.target_path.clone();
        let file = {
            let guard = self.index.read();
            loading_guard!(guard);
            guard.capture_shared_file(&params.0.path)
        };
        let file = match file {
            Some(f) => f,
            None => {
                return fail_and_return_mutation_replay(
                    &idempotency,
                    format::not_found_file(&params.0.path),
                );
            }
        };
        // Review finding 5 (post-v7.19.0): a rerouted edit must splice into
        // the worktree TARGET's current bytes. The index mirrors the indexed
        // copy (routed writes never touch it), so using index content as the
        // base would silently discard every earlier routed edit to this file.
        let edit_base = match edit::rebase_edit_base_for_reroute(file, &resolved_target) {
            Ok(base) => base,
            Err(e) => return fail_and_return_mutation_replay(&idempotency, e),
        };
        let source_authority = if edit_base.rebased {
            edit_format::EditSourceAuthority::WorktreeTarget
        } else {
            source_authority
        };
        let file = edit_base.file;
        if let Some(warning) = Self::check_edit_capability(
            &file.language,
            crate::parsing::config_extractors::EditCapability::TextEditSafe,
            "edit_within_symbol",
        ) {
            return fail_and_return_mutation_replay(&idempotency, warning);
        }
        let (_, sym) = match edit::resolve_or_error(
            &file,
            &params.0.name,
            params.0.kind.as_deref(),
            params.0.symbol_line,
        ) {
            Ok(s) => s,
            Err(e) => return fail_and_return_mutation_replay(&idempotency, e),
        };
        let evidence_anchor = symbol_anchor(&params.0.path, &sym);
        let sym_start = sym.effective_start() as usize;
        let sym_end = sym.byte_range.1 as usize;
        let body = &file.content[sym_start..sym_end];
        let body_str = match std::str::from_utf8(body) {
            Ok(s) => s,
            Err(_) => {
                return fail_and_return_mutation_replay(
                    &idempotency,
                    "Error: symbol body is not valid UTF-8.".to_string(),
                );
            }
        };
        // Normalize both old_text and new_text to match file line endings.
        let line_ending = edit::detect_line_ending(&file.content);
        let normalized_old =
            edit::normalize_line_endings(params.0.old_text.as_bytes(), line_ending);
        let normalized_old_str =
            String::from_utf8(normalized_old).unwrap_or_else(|_| params.0.old_text.clone());
        let normalized_new =
            edit::normalize_line_endings(params.0.new_text.as_bytes(), line_ending);
        let normalized_new_str =
            String::from_utf8(normalized_new).unwrap_or_else(|_| params.0.new_text.clone());
        let (new_body, count) = if params.0.replace_all {
            let count = body_str.matches(&normalized_old_str).count();
            if count > 0 {
                (
                    body_str.replace(&normalized_old_str, &normalized_new_str),
                    count,
                )
            } else {
                // Fallback: try whitespace-flexible matching.
                match edit::try_whitespace_flexible_replace(
                    body_str,
                    &normalized_old_str,
                    &normalized_new_str,
                    true,
                ) {
                    Some(result) => result,
                    None => (body_str.to_string(), 0), // hits count==0 error below
                }
            }
        } else {
            match body_str.find(&normalized_old_str) {
                Some(_) => (
                    body_str.replacen(&normalized_old_str, &normalized_new_str, 1),
                    1,
                ),
                None => {
                    // Fallback: try whitespace-flexible matching.
                    match edit::try_whitespace_flexible_replace(
                        body_str,
                        &normalized_old_str,
                        &normalized_new_str,
                        false,
                    ) {
                        Some(result) => result,
                        None => {
                            // Show a preview of the symbol body so the LLM can see what's actually there
                            let preview_len = 800.min(body_str.len());
                            let preview = &body_str[..preview_len];
                            let truncated = if preview_len < body_str.len() {
                                format!("\n... ({} more bytes)", body_str.len() - preview_len)
                            } else {
                                String::new()
                            };
                            let output = format!(
                                "Error: `{}` not found within symbol `{}`. \
                                 The symbol body is ({} bytes):\n```\n{}{}\n```",
                                params.0.old_text,
                                params.0.name,
                                body_str.len(),
                                preview,
                                truncated
                            );
                            return fail_and_return_mutation_replay(&idempotency, output);
                        }
                    }
                }
            }
        };
        if params.0.dry_run == Some(true) {
            if count == 0 {
                let preview_len = 800.min(body_str.len());
                let preview = &body_str[..preview_len];
                let truncated = if preview_len < body_str.len() {
                    format!("\n... ({} more bytes)", body_str.len() - preview_len)
                } else {
                    String::new()
                };
                let output = format!(
                    "Error: `{}` not found within symbol `{}`. \
                     The symbol body is ({} bytes):\n```\n{}{}\n```",
                    params.0.old_text,
                    params.0.name,
                    body_str.len(),
                    preview,
                    truncated
                );
                return fail_and_return_mutation_replay(&idempotency, output);
            }
            let mut result = format!(
                "{}\n[DRY RUN] Would edit within `{}` in {} ({} replacement(s))",
                edit_format::format_edit_envelope(
                    edit_format::EditSafetyMode::TextEditSafe,
                    source_authority,
                    edit_format::EditWriteSemantics::DryRunNoWrites,
                    &evidence_anchor,
                ),
                params.0.name,
                params.0.path,
                count
            );
            append_project_config_trust_suffix(&mut result, project_config_trust_suffix.as_deref());
            return result;
        }
        if count == 0 {
            let preview_len = 800.min(body_str.len());
            let preview = &body_str[..preview_len];
            let truncated = if preview_len < body_str.len() {
                format!("\n... ({} more bytes)", body_str.len() - preview_len)
            } else {
                String::new()
            };
            let output = format!(
                "Error: `{}` not found within symbol `{}`. \
                 The symbol body is ({} bytes):\n```\n{}{}\n```",
                params.0.old_text,
                params.0.name,
                body_str.len(),
                preview,
                truncated
            );
            return fail_and_return_mutation_replay(&idempotency, output);
        }
        let old_sym_bytes = sym_end - sym_start;
        let effective_range = (sym.effective_start(), sym.byte_range.1);
        let new_content = edit::apply_splice(&file.content, effective_range, new_body.as_bytes());
        let write_report = match edit::atomic_write_file(&resolved_path, &new_content) {
            Ok(report) => report,
            Err(e) => {
                let output = format!("Error writing {}: {e}", params.0.path);
                fail_mutation_replay(&idempotency, &output);
                return output;
            }
        };
        // Review finding 5 (post-v7.19.0): only a pass-through write updates
        // the index. A routed write leaves the indexed copy untouched, so
        // replacing the index entry with worktree bytes would make the index
        // lie about the indexed root — and the next edit's freshness check
        // would "correct" it back from disk, erasing the routed state it was
        // spliced from.
        if !resolved_target.rerouted {
            edit::reindex_after_write(
                &self.index,
                &resolved_path,
                &params.0.path,
                &new_content,
                file.language.clone(),
            );
        }
        edit_hooks::after_commit(&hook_ctx, &resolved_path);
        let mut out = format!(
            "{}\n{}",
            edit_format::format_edit_envelope(
                edit_format::EditSafetyMode::TextEditSafe,
                source_authority,
                edit_format::EditWriteSemantics::AtomicWriteAndReindex,
                &evidence_anchor,
            ),
            edit_format::format_edit_within(
                &params.0.path,
                &params.0.name,
                count,
                old_sym_bytes,
                new_body.len(),
            )
        );
        out.push_str(&edit_format::format_reroute_suffix(
            working_directory,
            &resolved_target,
        ));
        out.push_str(&edit::format_tee_snapshot_suffix(&write_report));
        append_project_config_trust_suffix(&mut out, project_config_trust_suffix.as_deref());
        self.append_impact_footer(&mut out, &params.0.path);
        complete_mutation_replay(&idempotency, &mut out);
        out
    }

    // ── Tier 2: Batch edit tools ──────────────────────────────────────────

    /// Apply multiple symbol-addressed edits atomically.
    /// Set dry_run=true for a read-only preview that makes no file changes.
    #[tool(
        name = "batch_edit",
        description = "Apply multiple symbol-addressed edits atomically across files. Each edit specifies a file, symbol, and operation (replace/insert_before/insert_after/delete/edit_within). Accepts either structured edits or shorthand strings like `src/lib.rs::helper => edit_within old >>> new`. All symbols are validated before any writes — if any resolution fails, no files are modified. Set dry_run=true for a READ-ONLY preview that shows what would change without writing (safe, no confirmation needed). Edits within the same file must target non-overlapping symbols. NOT for single-symbol edits (use replace_symbol_body, insert_symbol, etc.).",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    pub(crate) async fn batch_edit_tool(
        &self,
        params: Parameters<edit::BatchEditInput>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        let started = Instant::now();
        let dry_run = params.0.dry_run.unwrap_or(false);
        let operation_count = params.0.edits.len();
        let output = self.batch_edit(params).await;
        let status = classify_edit_output(&output, dry_run);
        let operation_statuses = match status {
            EditResultStatus::Success | EditResultStatus::DryRunSuccess => {
                success_operation_statuses(operation_count, status)
            }
            _ => failed_batch_operation_statuses(&output, status),
        };
        self.record_tool_completion(
            "batch_edit",
            &output,
            started.elapsed(),
            status.outcome_class(),
        );
        statused_edit_tool_result(output, status, operation_statuses)
    }

    pub(crate) async fn batch_edit(&self, params: Parameters<edit::BatchEditInput>) -> String {
        // N-6 (TR-06 boundary): the batch path carries NO `if_match`
        // optimistic-concurrency guard. `BatchEditInput` has no `if_match`
        // field and this executor performs no write-time guard re-read, so it
        // has the same TOCTOU window the single-symbol path closes. This is
        // intentionally NOT plumbed here (single-symbol fix lands first); if
        // `if_match` is ever extended to batch, it MUST re-verify at the write
        // like `guarded_atomic_write_file`, never become a silent false-safety
        // control. `guard_batch_reroute_divergence` (edit.rs) is a separate,
        // reroute-only divergence check — not an `if_match` write guard.
        if let Some(result) = self.proxy_tool_call("batch_edit", &params.0).await {
            return result;
        }
        self.note_worktree_misuse_if_active(params.0.working_directory.as_deref());
        {
            let guard = self.index.read();
            loading_guard!(guard);
        }
        let batch_paths: Vec<String> = params.0.edits.iter().map(|e| e.path.clone()).collect();
        let (repo_root, source_authority) = match prepare_batch_paths_for_edit(self, &batch_paths) {
            Ok(prepared) => prepared,
            Err(e) => return e,
        };
        let project_config_trust_suffix = match project_config_trust_response_suffix(&repo_root) {
            Ok(suffix) => suffix,
            Err(message) => return message,
        };
        let dry_run = params.0.dry_run.unwrap_or(false);
        let idempotency = match begin_mutation_replay(
            &repo_root,
            "batch_edit",
            &params.0,
            params.0.idempotency_key.as_deref(),
            dry_run,
        ) {
            Ok(idempotency) => idempotency,
            Err(output) => return output,
        };
        match edit::execute_batch_edit(
            &self.index,
            &repo_root,
            &params.0.edits,
            dry_run,
            params
                .0
                .working_directory
                .as_deref()
                .map(std::path::Path::new),
        ) {
            Ok(summaries) => {
                let file_count = params
                    .0
                    .edits
                    .iter()
                    .map(|e| e.path.as_str())
                    .collect::<std::collections::HashSet<_>>()
                    .len();
                let write_semantics = if dry_run {
                    edit_format::EditWriteSemantics::DryRunNoWrites
                } else {
                    edit_format::EditWriteSemantics::TransactionalWriteRollbackAndReindex
                };
                let evidence = format!(
                    "{} edit target(s) across {} file(s)",
                    params.0.edits.len(),
                    file_count
                );
                let mut result = format!(
                    "{}\n{}",
                    edit_format::format_batch_envelope(
                        edit_format::EditSafetyMode::StructuralEditSafe,
                        edit_format::MatchType::Exact,
                        source_authority,
                        write_semantics,
                        &evidence,
                    ),
                    edit_format::format_batch_summary(&summaries, file_count),
                );
                append_project_config_trust_suffix(
                    &mut result,
                    project_config_trust_suffix.as_deref(),
                );
                if let Some(primary) = params.0.edits.first() {
                    self.append_impact_footer(&mut result, &primary.path);
                }
                complete_mutation_replay(&idempotency, &mut result);
                result
            }
            Err(e) => {
                fail_mutation_replay(&idempotency, &e);
                e
            }
        }
    }

    /// Rename a symbol and update all references project-wide.
    /// Set dry_run=true for a read-only preview that makes no file changes.
    #[tool(
        description = "Rename a symbol and update all references across the project. Finds the definition and all usage sites via the index's reverse reference map. Set dry_run=true for a READ-ONLY preview that lists affected files without writing any changes (safe, no confirmation needed). Applies confident matches transactionally across files; uncertain matches are surfaced for manual review instead of being modified. Common names (e.g. `new`, `get`) can still produce false positives — verify with what_changed afterward. NOT for replacing a symbol's body (use replace_symbol_body).",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    pub(crate) async fn batch_rename(&self, params: Parameters<edit::BatchRenameInput>) -> String {
        // N-6 (TR-06 boundary): no `if_match` guard on the batch path (same
        // TOCTOU window as batch_edit if extended). See the note on
        // `batch_edit`. Not plumbed here by design.
        if let Some(result) = self.proxy_tool_call("batch_rename", &params.0).await {
            return result;
        }
        self.note_worktree_misuse_if_active(params.0.working_directory.as_deref());
        let repo_root = match self.capture_repo_root() {
            Some(root) => root,
            None => return "Error: no repository root configured.".to_string(),
        };
        let project_config_trust_suffix = match project_config_trust_response_suffix(&repo_root) {
            Ok(suffix) => suffix,
            Err(message) => return message,
        };
        {
            let guard = self.index.read();
            loading_guard!(guard);
        }
        let source_authority = prepare_project_wide_rename(self, &repo_root);
        let dry_run = params.0.dry_run.unwrap_or(false);
        let idempotency = match begin_mutation_replay(
            &repo_root,
            "batch_rename",
            &params.0,
            params.0.idempotency_key.as_deref(),
            dry_run,
        ) {
            Ok(idempotency) => idempotency,
            Err(output) => return output,
        };
        match edit::execute_batch_rename(&self.index, &repo_root, &params.0) {
            Ok(summary) => {
                let write_semantics = if dry_run {
                    edit_format::EditWriteSemantics::DryRunNoWrites
                } else {
                    edit_format::EditWriteSemantics::TransactionalWriteRollbackAndReindex
                };
                let evidence = format!(
                    "definition `{}` + project-wide constrained references",
                    params.0.path
                );
                let mut result = format!(
                    "{}\n{}",
                    edit_format::format_batch_envelope(
                        edit_format::EditSafetyMode::StructuralEditSafe,
                        edit_format::MatchType::Constrained,
                        source_authority,
                        write_semantics,
                        &evidence,
                    ),
                    summary,
                );
                append_project_config_trust_suffix(
                    &mut result,
                    project_config_trust_suffix.as_deref(),
                );
                self.append_impact_footer(&mut result, &params.0.path);
                complete_mutation_replay(&idempotency, &mut result);
                result
            }
            Err(e) => {
                fail_mutation_replay(&idempotency, &e);
                e
            }
        }
    }

    /// Insert the same code at multiple symbol locations across files.
    #[tool(
        name = "batch_insert",
        description = "Insert the same code before or after multiple symbols across the project. Useful for adding logging, instrumentation, or boilerplate to many locations at once. Accepts either structured targets or shorthand strings like `src/lib.rs::helper`. Code is auto-indented to match each target symbol. All targets are validated before any writes, and live execution applies transactionally across files with rollback on failure. Set dry_run=true for a READ-ONLY preview. NOT for inserting at a single location (use insert_symbol).",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    pub(crate) async fn batch_insert_tool(
        &self,
        params: Parameters<edit::BatchInsertInput>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        let started = Instant::now();
        let dry_run = params.0.dry_run.unwrap_or(false);
        let operation_count = params.0.targets.len();
        let output = self.batch_insert(params).await;
        let status = classify_edit_output(&output, dry_run);
        let operation_statuses = match status {
            EditResultStatus::Success | EditResultStatus::DryRunSuccess => {
                success_operation_statuses(operation_count, status)
            }
            _ => failed_batch_operation_statuses(&output, status),
        };
        self.record_tool_completion(
            "batch_insert",
            &output,
            started.elapsed(),
            status.outcome_class(),
        );
        statused_edit_tool_result(output, status, operation_statuses)
    }

    pub(crate) async fn batch_insert(&self, params: Parameters<edit::BatchInsertInput>) -> String {
        // N-6 (TR-06 boundary): no `if_match` guard on the batch path (same
        // TOCTOU window as batch_edit if extended). See the note on
        // `batch_edit`. Not plumbed here by design.
        if let Some(result) = self.proxy_tool_call("batch_insert", &params.0).await {
            return result;
        }
        self.note_worktree_misuse_if_active(params.0.working_directory.as_deref());
        {
            let guard = self.index.read();
            loading_guard!(guard);
        }
        let batch_paths: Vec<String> = params.0.targets.iter().map(|t| t.path.clone()).collect();
        let (repo_root, source_authority) = match prepare_batch_paths_for_edit(self, &batch_paths) {
            Ok(prepared) => prepared,
            Err(e) => return e,
        };
        let project_config_trust_suffix = match project_config_trust_response_suffix(&repo_root) {
            Ok(suffix) => suffix,
            Err(message) => return message,
        };
        let dry_run = params.0.dry_run.unwrap_or(false);
        let idempotency = match begin_mutation_replay(
            &repo_root,
            "batch_insert",
            &params.0,
            params.0.idempotency_key.as_deref(),
            dry_run,
        ) {
            Ok(idempotency) => idempotency,
            Err(output) => return output,
        };
        match edit::execute_batch_insert(&self.index, &repo_root, &params.0) {
            Ok(summaries) => {
                let file_count = params
                    .0
                    .targets
                    .iter()
                    .map(|t| t.path.as_str())
                    .collect::<std::collections::HashSet<_>>()
                    .len();
                let write_semantics = if dry_run {
                    edit_format::EditWriteSemantics::DryRunNoWrites
                } else {
                    edit_format::EditWriteSemantics::TransactionalWriteRollbackAndReindex
                };
                let evidence = format!(
                    "{} target(s) across {} file(s)",
                    params.0.targets.len(),
                    file_count
                );
                let mut result = format!(
                    "{}\n{}",
                    edit_format::format_batch_envelope(
                        edit_format::EditSafetyMode::StructuralEditSafe,
                        edit_format::MatchType::Exact,
                        source_authority,
                        write_semantics,
                        &evidence,
                    ),
                    edit_format::format_batch_summary(&summaries, file_count),
                );
                append_project_config_trust_suffix(
                    &mut result,
                    project_config_trust_suffix.as_deref(),
                );
                if let Some(primary) = params.0.targets.first() {
                    self.append_impact_footer(&mut result, &primary.path);
                }
                complete_mutation_replay(&idempotency, &mut result);
                result
            }
            Err(e) => {
                fail_mutation_replay(&idempotency, &e);
                e
            }
        }
    }
}
