//! `secret_remediate` — agent-driven secret remediation (feature 034).
//!
//! Preview defaults to true. Apply is a write tool (harness permission gate).
//! Never returns secret bytes or private keys.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::edit_safety::batch_commit::{
    BatchIo, EmbeddedBatchIo, StagedImage, commit_staged_locked, with_staged_locks,
};
use crate::edit_safety::secret_remediation as shared_stage;
use crate::knowledge;
use crate::knowledge::secret_remediation::{self, SelectionRefusal};
use crate::protocol::SymForgeServer;
use crate::protocol::edit_tools::fail_and_return_mutation_replay;
use crate::protocol::result_status::{OutcomeClass, ResultStatus};
use crate::protocol::secret_dismissals::DISMISSAL_STORE_REL;
use crate::protocol::withheld::RemediationActionName;

const MAX_REMEDIATION_FILE_BYTES: usize = 2 * 1024 * 1024;
const MAX_REMEDIATION_TOTAL_BYTES: usize = 64 * 1024 * 1024;
const MAX_REMEDIATION_SCAN_FILES: usize = 4096;

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SecretRemediateInput {
    /// One path, list of paths, or the string `"repo"`.
    pub scope: Value,
    /// Required non-empty finding ids from `_meta["symforge/withheld"]`.
    pub finding_ids: Vec<String>,
    /// `externalize` | `encrypt` | `dismiss`
    pub action: String,
    /// Default true. `false` selects apply (write).
    #[serde(default = "default_preview_true")]
    pub preview: bool,
    /// Honored on apply like the edit lane (durable project-state replay).
    pub idempotency_key: Option<String>,
}

fn default_preview_true() -> bool {
    true
}

#[tool_router(router = secret_remediate_tool_router, vis = "pub(crate)")]
impl SymForgeServer {
    #[tool(
        name = "secret_remediate",
        description = "Preview or apply secret remediation (externalize/encrypt/dismiss). Preview defaults true; apply requires write permission. Never returns secret bytes.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    pub(crate) async fn secret_remediate_tool(
        &self,
        params: Parameters<SecretRemediateInput>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        let started = Instant::now();
        let input = params.0;
        let preview = input.preview;
        let result = self.secret_remediate_inner(input).await;
        let text = match &result {
            Ok(t) | Err(t) => t.clone(),
        };
        let outcome = if result.is_ok() {
            if text.contains("apply_status: incomplete") {
                OutcomeClass::Ambiguous
            } else {
                OutcomeClass::Found
            }
        } else if text.starts_with("Error:") {
            OutcomeClass::InvalidRequest
        } else {
            OutcomeClass::InternalFailure
        };
        self.record_tool_completion("secret_remediate", &text, started.elapsed(), outcome);
        if preview {
            Ok(ResultStatus::new(outcome).into_call_tool_result(text))
        } else {
            Ok(ResultStatus::new(outcome).into_mutation_call_tool_result(text))
        }
    }
}

impl SymForgeServer {
    async fn secret_remediate_inner(&self, input: SecretRemediateInput) -> Result<String, String> {
        if input.finding_ids.is_empty() {
            return Err(
                "Error: finding_ids must be a non-empty array (empty means reject; no implicit all-in-scope)"
                    .to_string(),
            );
        }
        let action = match input.action.as_str() {
            "externalize" => RemediationActionName::Externalize,
            "encrypt" => RemediationActionName::Encrypt,
            "dismiss" => RemediationActionName::Dismiss,
            other => {
                return Err(format!(
                    "Error: action must be externalize|encrypt|dismiss, got {other:?}"
                ));
            }
        };

        if !input.preview && input.idempotency_key.as_deref().is_none_or(str::is_empty) {
            return Err("Error: apply requires a non-empty idempotency_key".to_string());
        }
        // Idempotency: preview never reserves; apply honors edit-lane replay.
        let idempotency = match begin_secret_replay(self, &input) {
            Ok(active) => active,
            Err(output) => {
                if output.starts_with("Error:") {
                    return Err(output);
                }
                return Ok(output);
            }
        };

        let paths = match resolve_scope_paths(self, &input.scope) {
            Ok(p) => p,
            Err(e) => return Err(fail_and_return_mutation_replay(&idempotency, e)),
        };

        match action {
            RemediationActionName::Externalize => {
                self.run_externalize(&input, &paths, &idempotency).await
            }
            RemediationActionName::Encrypt => self.run_encrypt(&input, &paths, &idempotency).await,
            RemediationActionName::Dismiss => self.run_dismiss(&input, &paths, &idempotency).await,
        }
    }

    async fn run_externalize(
        &self,
        input: &SecretRemediateInput,
        paths: &[String],
        idempotency: &Option<crate::idempotency::ActiveReplay>,
    ) -> Result<String, String> {
        let (root, generation, state_dir) = planning_binding(self)?;
        let selected = select_bounded(&root, paths, &input.finding_ids, "externalize")
            .map_err(|error| fail_and_return_mutation_replay(idempotency, error))?;

        if input.preview {
            let plans = secret_remediation::plan_externalize(selected);
            let mut out = String::from("secret_remediate preview (externalize)\n");
            out.push_str("Files that would be created or modified:\n");
            for plan in &plans {
                out.push_str(&format!("- {}\n", plan.path));
            }
            out.push_str("- .env\n");
            out.push_str("- .gitignore (ensure .env entry)\n");
            out.push_str("\nMasked diff:\n");
            for plan in plans {
                out.push_str(&plan.masked_diff);
                out.push('\n');
            }
            return Ok(out);
        }

        let selected_paths = selected
            .iter()
            .map(|file| file.path.clone())
            .collect::<Vec<_>>();
        let staged = shared_stage::stage_externalize(&root, selected, |path| {
            crate::index_lifecycle::physical_root::read_regular_beneath_root(
                &root,
                Path::new(path),
                MAX_REMEDIATION_FILE_BYTES,
            )
            .map_err(|_| shared_stage::StageError::SourceUnavailable)
        })
        .map_err(|_| {
            fail_and_return_mutation_replay(
                idempotency,
                "Error: externalize staging refused".into(),
            )
        })?;
        let source = guarded_commit(
            self,
            &root,
            generation,
            state_dir.as_deref(),
            &staged,
            idempotency,
        )?;
        let mut rescan = String::from("clean");
        for path in &selected_paths {
            match source.read_regular_beneath_anchor(Path::new(path), MAX_REMEDIATION_FILE_BYTES) {
                Ok(Some(bytes)) => match knowledge::scan_secret_bytes(path, &bytes) {
                    knowledge::SecretScan::Clean => {}
                    knowledge::SecretScan::Sensitive { finding_count, .. } => {
                        rescan = format!("still_sensitive finding_count={finding_count}");
                    }
                    knowledge::SecretScan::Indeterminate { reason } => {
                        rescan = format!("indeterminate {reason:?}");
                    }
                },
                _ => rescan = "indeterminate source_unavailable".into(),
            }
        }
        let mut out = format!(
            "secret_remediate apply (externalize)\n{}\nwritten:\n",
            apply_status_for_rescan(&rescan)
        );
        for image in &staged {
            out.push_str(&format!("- {}\n", image.relative.display()));
        }
        out.push_str(&format!("rescan: {rescan}\n"));
        complete_guarded_replay(idempotency, &source, &staged, &mut out)?;
        let _ = crate::watcher::reconcile_stale_files(&root, self.index.data_plane());
        Ok(out)
    }

    async fn run_encrypt(
        &self,
        input: &SecretRemediateInput,
        paths: &[String],
        idempotency: &Option<crate::idempotency::ActiveReplay>,
    ) -> Result<String, String> {
        let availability = encrypt_runtime_availability(self);
        if !availability.0 {
            return Err(fail_and_return_mutation_replay(
                idempotency,
                format!("Error: encrypt is unavailable ({})", availability.1),
            ));
        }
        let binary = sops_binary_on_path().ok_or_else(|| {
            fail_and_return_mutation_replay(
                idempotency,
                "Error: encrypt is unavailable (sops_not_installed)".to_string(),
            )
        })?;
        let (root, generation, state_dir) = planning_binding(self)?;
        let selected = select_bounded(&root, paths, &input.finding_ids, "encrypt")
            .map_err(|error| fail_and_return_mutation_replay(idempotency, error))?;
        secret_remediation::ensure_encrypt_formats(&selected).map_err(|error| {
            fail_and_return_mutation_replay(idempotency, selection_error_text(error, "encrypt"))
        })?;
        if input.preview {
            let mut out = String::from(
                "secret_remediate preview (encrypt)\nFiles that would be created or modified:\n",
            );
            for file in &selected {
                out.push_str(&format!("- {}\n", file.path));
            }
            out.push_str("\nMasked summary:\n");
            for file in &selected {
                out.push_str(&format!(
                    "- {}: SOPS encryption with age recipient (public only)\n",
                    file.path
                ));
            }
            return Ok(out);
        }

        let selected_paths = selected
            .iter()
            .map(|file| file.path.clone())
            .collect::<Vec<_>>();
        let deadline = Instant::now() + std::time::Duration::from_secs(300);
        let mut staged = Vec::with_capacity(selected.len());
        for file in selected {
            let timeout = deadline.saturating_duration_since(Instant::now());
            if timeout.is_zero() {
                return Err(fail_and_return_mutation_replay(
                    idempotency,
                    "Error: encrypt preparation deadline exceeded".to_string(),
                ));
            }
            let encrypted = shared_stage::encrypt_selected_bytes(
                &file.path,
                &file.original,
                &binary,
                &availability.1,
                timeout,
                || None,
            )
            .map_err(|reason| {
                fail_and_return_mutation_replay(
                    idempotency,
                    format!("Error: encrypt preparation refused ({reason:?})"),
                )
            })?;
            if secret_remediation::contains_selected_plaintext(&encrypted, &file) {
                return Err(fail_and_return_mutation_replay(
                    idempotency,
                    "Error: encrypt output retained selected plaintext".to_string(),
                ));
            }
            staged.push(StagedImage {
                absolute: root.join(&file.path),
                relative: PathBuf::from(&file.path),
                original: Some(file.original),
                replacement: encrypted,
                owner_only: false,
            });
        }
        let source = guarded_commit(
            self,
            &root,
            generation,
            state_dir.as_deref(),
            &staged,
            idempotency,
        )?;
        let mut rescan = String::from("clean");
        let mut details = String::new();
        for path in &selected_paths {
            let finding = match source
                .read_regular_beneath_anchor(Path::new(path), MAX_REMEDIATION_FILE_BYTES)
            {
                Ok(Some(bytes)) => match knowledge::scan_secret_bytes(path, &bytes) {
                    knowledge::SecretScan::Clean => "clean".to_string(),
                    knowledge::SecretScan::Sensitive { finding_count, .. } => {
                        format!("still_sensitive finding_count={finding_count}")
                    }
                    knowledge::SecretScan::Indeterminate { reason } => {
                        format!("indeterminate {reason:?}")
                    }
                },
                _ => "indeterminate source_unavailable".to_string(),
            };
            if finding != "clean" {
                rescan = finding.clone();
            }
            details.push_str(&format!("rescan ({path}) : {finding}\n"));
        }
        let mut out = format!(
            "secret_remediate apply (encrypt)\n{}\nwritten:\n",
            apply_status_for_rescan(&rescan)
        );
        for image in &staged {
            out.push_str(&format!("- {}\n", image.relative.display()));
        }
        out.push_str(&details);
        out.push_str("plaintext_absent: true\n");
        out.push_str(&history_note(&root));
        out.push('\n');
        complete_guarded_replay(idempotency, &source, &staged, &mut out)?;
        let _ = crate::watcher::reconcile_stale_files(&root, self.index.data_plane());
        Ok(out)
    }

    async fn run_dismiss(
        &self,
        input: &SecretRemediateInput,
        paths: &[String],
        idempotency: &Option<crate::idempotency::ActiveReplay>,
    ) -> Result<String, String> {
        let (root, generation, state_dir) = planning_binding(self)?;
        let selected = select_bounded(&root, paths, &input.finding_ids, "dismiss")
            .map_err(|error| fail_and_return_mutation_replay(idempotency, error))?;
        let records = secret_remediation::plan_dismiss(&selected).map_err(|error| {
            fail_and_return_mutation_replay(idempotency, selection_error_text(error, "dismiss"))
        })?;
        if input.preview {
            let mut out = String::from(
                "secret_remediate preview (dismiss)\nFiles that would be created or modified:\n",
            );
            out.push_str(&format!("- {DISMISSAL_STORE_REL}\n\nMasked summary:\n"));
            for record in &records {
                out.push_str(&format!(
                    "- {} rule={} digest={}\n",
                    record.path,
                    record.rule_id,
                    &record.line_digest[..12.min(record.line_digest.len())],
                ));
            }
            return Ok(out);
        }
        let selected_paths = selected
            .iter()
            .map(|file| file.path.clone())
            .collect::<Vec<_>>();
        let staged = shared_stage::stage_dismiss(&root, &selected, |path| {
            crate::index_lifecycle::physical_root::read_regular_beneath_root(
                &root,
                Path::new(path),
                MAX_REMEDIATION_FILE_BYTES,
            )
            .map_err(|_| shared_stage::StageError::SourceUnavailable)
        })
        .map_err(|error| {
            let reason = match error {
                shared_stage::StageError::SourceUnavailable => {
                    "Error: dismiss apply refused; dismissal store could not be loaded".to_string()
                }
                _ => format!("Error: dismiss staging refused ({error:?})"),
            };
            fail_and_return_mutation_replay(idempotency, reason)
        })?;
        let source = guarded_commit(
            self,
            &root,
            generation,
            state_dir.as_deref(),
            &staged,
            idempotency,
        )?;
        let actual_store = source
            .read_regular_beneath_anchor(Path::new(DISMISSAL_STORE_REL), MAX_REMEDIATION_FILE_BYTES)
            .ok()
            .flatten();
        let current_records = actual_store.as_deref().and_then(|bytes| {
            crate::knowledge::secret_dismissals::parse_dismissals_bytes(bytes).ok()
        });
        use crate::live_index::single_file::{
            ReindexOutcome, admit_and_index_single_path, reconcile_secret_dismissals,
        };
        let shared = self.index.data_plane();
        let mut outcomes = reconcile_secret_dismissals(shared, &root);
        let mut rescan = String::from("clean");
        let mut details = String::new();
        for path in &selected_paths {
            let absolute = root.join(path);
            let readmit = || {
                admit_and_index_single_path(
                    path,
                    &absolute,
                    shared,
                    shared.current_project_generation(),
                )
            };
            let mut outcome = outcomes.remove(path).unwrap_or_else(readmit);
            if matches!(outcome, ReindexOutcome::PublicationRejected) {
                outcome = readmit();
            }
            let index_outcome = match outcome {
                ReindexOutcome::Reindexed | ReindexOutcome::HashSkip => "indexed",
                ReindexOutcome::Skipped => "withheld",
                ReindexOutcome::NotFound | ReindexOutcome::Removed => "absent",
                ReindexOutcome::ReadError(_) => "unreadable",
                ReindexOutcome::PublicationRejected => {
                    "publication rejected by a concurrent index change; index unchanged"
                }
            };
            let finding = match (
                &current_records,
                source.read_regular_beneath_anchor(Path::new(path), MAX_REMEDIATION_FILE_BYTES),
            ) {
                (Some(records), Ok(Some(bytes))) => {
                    match crate::knowledge::secret_dismissals::scan_with_records(
                        path, &bytes, records,
                    ) {
                        knowledge::SecretScan::Clean => "clean".to_string(),
                        knowledge::SecretScan::Sensitive { finding_count, .. } => {
                            format!("still_sensitive finding_count={finding_count}")
                        }
                        knowledge::SecretScan::Indeterminate { reason } => {
                            format!("indeterminate {reason:?}")
                        }
                    }
                }
                _ => "indeterminate source_unavailable".to_string(),
            };
            if finding != "clean" {
                rescan = finding.clone();
            }
            details.push_str(&format!(
                "rescan ({path}) : {finding}\nindex ({path}) : {index_outcome}\n"
            ));
        }
        let mut out = format!(
            "secret_remediate apply (dismiss)\n{}\nwritten:\n- {DISMISSAL_STORE_REL}\n",
            apply_status_for_rescan(&rescan)
        );
        out.push_str(&details);
        out.push_str(&history_note(&root));
        out.push('\n');
        complete_guarded_replay(idempotency, &source, &staged, &mut out)?;
        Ok(out)
    }
}

fn admitted_root_matches(
    root: &Path,
    source: &crate::index_lifecycle::activation::ProjectSourceAuthority,
) -> bool {
    root.canonicalize()
        .is_ok_and(|canonical| canonical == source.admitted_root())
}

/// Reconcile the server's resolved state placement with the pinned index
/// binding for a replay. The roots must canonicalize equal, and two present
/// state dirs must agree. When only one side carries a state dir, that one is
/// the durable placement for this root. `label` names the caller in refusals.
pub(crate) fn resolved_project_state(
    server: &SymForgeServer,
    root: &Path,
    runtime: Option<&crate::domain::ProjectStateDir>,
    label: &str,
) -> Result<Option<crate::domain::ProjectStateDir>, String> {
    let unavailable = || format!("Error: admitted {label} source unavailable");
    let server_root = server.capture_repo_root().ok_or_else(unavailable)?;
    let (Ok(server_canonical), Ok(runtime_canonical)) =
        (server_root.canonicalize(), root.canonicalize())
    else {
        return Err(unavailable());
    };
    if server_canonical != runtime_canonical {
        return Err(format!("Error: admitted {label} source changed"));
    }
    let server_state = server.capture_project_state_dir();
    if let (Some(runtime), Some(server_state)) = (runtime, server_state.as_ref())
        && runtime.as_path() != server_state.as_path()
    {
        return Err("Error: durable project-state placement changed".to_string());
    }
    Ok(server_state.or_else(|| runtime.cloned()))
}

fn begin_secret_replay(
    server: &SymForgeServer,
    input: &SecretRemediateInput,
) -> Result<Option<crate::idempotency::ActiveReplay>, String> {
    if input.preview {
        return Ok(None);
    }
    let raw_key = input
        .idempotency_key
        .as_deref()
        .filter(|key| !key.is_empty())
        .ok_or_else(|| "Error: apply requires a non-empty idempotency_key".to_string())?;
    let mut request = serde_json::to_value(input)
        .map_err(|_| "Error: remediation request cannot be serialized".to_string())?;
    if let Value::Object(map) = &mut request {
        map.remove("idempotency_key");
    }
    let decision = server
        .index
        .with_admitted_replay_source(|root, state, source| {
            if !admitted_root_matches(root, source) {
                return Err("Error: admitted remediation source changed".to_string());
            }
            let placement = resolved_project_state(server, root, state, "remediation")?;
            let state = placement.as_ref().ok_or_else(|| {
                "Error: durable project-state replay is unavailable for this binding".to_string()
            })?;
            crate::idempotency::begin_tool_replay_verified_bound(
                state,
                "secret_remediate",
                raw_key,
                &request,
                source,
            )
            .map_err(|error| crate::idempotency::format_tool_error(&error))
        })
        .map_err(|_| "Error: admitted remediation source unavailable".to_string())?
        .ok_or_else(|| "Error: admitted remediation source unavailable".to_string())??;
    match decision {
        crate::idempotency::ReplayStart::FirstExecution(active) => Ok(Some(active)),
        crate::idempotency::ReplayStart::Replay(response) => {
            if response.starts_with("Idempotency replay unavailable:") {
                Err(format!("Error: {response}"))
            } else {
                Err(response)
            }
        }
    }
}

fn planning_binding(server: &SymForgeServer) -> Result<(PathBuf, u64, Option<PathBuf>), String> {
    let shared = server.index.data_plane();
    let live = shared.read();
    let root = live
        .indexed_root
        .clone()
        .ok_or_else(|| "Error: no indexed root bound".to_string())?;
    let generation = shared.current_project_generation();
    let state_dir = resolved_project_state(
        server,
        &root,
        shared.project_state_dir().as_deref(),
        "remediation",
    )?
    .map(|state| state.as_path().to_path_buf());
    Ok((root, generation, state_dir))
}

fn select_bounded(
    root: &Path,
    paths: &[String],
    finding_ids: &[String],
    action: &str,
) -> Result<Vec<secret_remediation::SelectedFile>, String> {
    if paths.len() > MAX_REMEDIATION_SCAN_FILES {
        return Err(selection_error_text(
            SelectionRefusal::ResourceLimit,
            action,
        ));
    }
    let mut total = 0usize;
    secret_remediation::select_requested_checked(paths, finding_ids, |path| {
        let bytes = crate::index_lifecycle::physical_root::read_regular_beneath_root(
            root,
            Path::new(path),
            MAX_REMEDIATION_FILE_BYTES,
        )
        .map_err(|_| SelectionRefusal::ResourceLimit)?;
        if let Some(bytes) = &bytes {
            total = total.saturating_add(bytes.len());
            if total > MAX_REMEDIATION_TOTAL_BYTES {
                return Err(SelectionRefusal::ResourceLimit);
            }
        }
        Ok(bytes)
    })
    .map_err(|error| selection_error_text(error, action))
}

fn guarded_commit(
    server: &SymForgeServer,
    planned_root: &Path,
    planned_generation: u64,
    planned_state: Option<&Path>,
    staged: &[StagedImage],
    idempotency: &Option<crate::idempotency::ActiveReplay>,
) -> Result<Arc<crate::index_lifecycle::activation::ProjectSourceAuthority>, String> {
    with_staged_locks(staged, |order| {
        server.index.with_admitted_write_binding(
            |root, state, generation, authority, publication| {
                if root != planned_root
                    || !admitted_root_matches(planned_root, authority)
                    || generation != planned_generation
                    || state
                        .is_some_and(|state| Some(state.as_path()) != planned_state)
                    || staged.iter().any(|image| image.absolute != root.join(&image.relative))
                {
                    return Err("Error: source binding changed before remediation apply".to_string());
                }
                let write = authority.acquire_write_expected(publication).map_err(|_| {
                    "Error: source publication changed before remediation apply".to_string()
                })?;
                let mut io = EmbeddedBatchIo::new(write);
                for image in staged {
                    match io.matches(image, image.original.as_deref()) {
                        Ok(true) => {}
                        Ok(false) => {
                            let _ = io.finish();
                            return Err("Error: remediation source bytes changed before apply".to_string());
                        }
                        Err(_) => {
                            let _ = io.finish();
                            return Err("Error: remediation source preimage unavailable".to_string());
                        }
                    }
                }
                if let Some(active) = idempotency
                    && active.mark_started().is_err() {
                        let _ = io.finish();
                        return Err("Error: durable remediation start record unavailable".to_string());
                    }
                let committed = commit_staged_locked(staged, order, &mut io, None);
                let finished = io.finish();
                if committed.is_err() || finished.is_err() {
                    if let Some(active) = idempotency {
                        let _ = active.mark_uncertain();
                    }
                    return Err("secret_remediate apply\napply_status: incomplete\nreason: guarded_write_uncertain\n".to_string());
                }
                Ok(Arc::clone(authority))
            },
        )
    })
    .map_err(|_| "Error: remediation target lock unavailable".to_string())?
    .map_err(|_| "Error: admitted source binding unavailable".to_string())?
    .ok_or_else(|| "Error: admitted source is not current".to_string())?
}

fn complete_guarded_replay(
    idempotency: &Option<crate::idempotency::ActiveReplay>,
    source: &crate::index_lifecycle::activation::ProjectSourceAuthority,
    staged: &[StagedImage],
    output: &mut String,
) -> Result<(), String> {
    let Some(active) = idempotency else {
        return Err(
            "secret_remediate apply\napply_status: incomplete\nreason: missing_replay_lease\n"
                .to_string(),
        );
    };
    let targets = staged
        .iter()
        .map(|image| crate::idempotency::PostImageTarget {
            path: image.absolute.display().to_string(),
            content_digest: Some(crate::hash::digest_hex(&image.replacement)),
        })
        .collect();
    let receipt = crate::idempotency::PostImageReceipt {
        targets,
        source: None,
    };
    let Some(receipt) = crate::idempotency::bind_post_image_to_source(receipt, source) else {
        let _ = active.mark_uncertain();
        return Err("secret_remediate apply\napply_status: incomplete\nreason: source_post_image_unavailable\n".to_string());
    };
    if active
        .complete_with_post_image(output.clone(), Some(receipt))
        .is_err()
    {
        let _ = active.mark_uncertain();
        output.clear();
        output.push_str("secret_remediate apply\napply_status: incomplete\nreason: replay_completion_unavailable\n");
        return Err(output.clone());
    }
    Ok(())
}

/// Returns (available, reason_or_recipient).
fn encrypt_runtime_availability(server: &SymForgeServer) -> (bool, String) {
    if sops_binary_on_path().is_none() {
        return (false, "sops_not_installed".to_string());
    }
    let live = server.index.data_plane().read();
    let Some(root) = live.indexed_root.as_deref() else {
        return (false, "no_indexed_root".to_string());
    };
    match resolve_age_recipient(root) {
        Ok(r) => (true, r),
        Err(reason) => (false, reason),
    }
}

fn sops_binary_on_path() -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths).find_map(|dir| {
            let candidate = dir.join(if cfg!(windows) { "sops.exe" } else { "sops" });
            candidate
                .is_file()
                .then(|| std::fs::canonicalize(candidate).ok())
                .flatten()
        })
    })
}

fn resolve_age_recipient(root: &Path) -> Result<String, String> {
    if let Ok(env) = std::env::var("SOPS_AGE_RECIPIENTS") {
        let first = env
            .split(',')
            .map(str::trim)
            .find(|s| !s.is_empty() && s.starts_with("age1"));
        if let Some(r) = first {
            return Ok(r.to_string());
        }
    }
    let sops_yaml = root.join(".sops.yaml");
    if !sops_yaml.is_file() {
        return Err("no_recipient_configured".to_string());
    }
    let text = std::fs::read_to_string(&sops_yaml).map_err(|_| "no_recipient_configured")?;
    for token in text.split_whitespace() {
        let t = token.trim_matches(|c: char| c == '"' || c == '\'' || c == ',');
        if t.starts_with("age1") && t.len() > 16 {
            return Ok(t.to_string());
        }
    }
    Err("no_recipient_configured".to_string())
}

fn resolve_scope_paths(server: &SymForgeServer, scope: &Value) -> Result<Vec<String>, String> {
    let live = server.index.data_plane().read();
    let Some(root) = live.indexed_root.as_deref() else {
        return Err("Error: no indexed root bound".to_string());
    };
    match scope {
        Value::String(s) if s == "repo" => {
            let mut paths: Vec<String> = live
                .all_files()
                .map(|(path, _)| path.replace('\\', "/"))
                .collect();
            paths.sort();
            paths.dedup();
            Ok(paths)
        }
        Value::String(s) => {
            let norm = s.replace('\\', "/");
            crate::discovery::resolve_repo_path(root, &norm)
                .map_err(|e| format!("Error: scope path refused: {e:?}"))?
                .ok_or_else(|| format!("Error: scope path not found: {norm}"))?;
            Ok(vec![norm])
        }
        Value::Array(arr) => {
            let mut out = Vec::new();
            for v in arr {
                let Some(s) = v.as_str() else {
                    return Err("Error: scope array entries must be strings".to_string());
                };
                let norm = s.replace('\\', "/");
                crate::discovery::resolve_repo_path(root, &norm)
                    .map_err(|e| format!("Error: scope path refused: {e:?}"))?
                    .ok_or_else(|| format!("Error: scope path not found: {norm}"))?;
                out.push(norm);
            }
            Ok(out)
        }
        _ => Err("Error: scope must be a path string, path array, or \"repo\"".to_string()),
    }
}

fn selection_error_text(error: SelectionRefusal, action: &str) -> String {
    match error {
        SelectionRefusal::EmptyFindings => "Error: finding_ids must be non-empty".to_owned(),
        SelectionRefusal::DuplicateFinding => "Error: duplicate finding_id".to_owned(),
        SelectionRefusal::TooManyFindings => "Error: too many finding_ids".to_owned(),
        SelectionRefusal::ScanIndeterminate => {
            format!("Error: secret span scan indeterminate; refusing {action}")
        }
        SelectionRefusal::MissingFinding => "Error: finding_ids not found in scope".to_owned(),
        SelectionRefusal::UndismissableFinding => {
            "Error: finding rule cannot be dismissed".to_owned()
        }
        SelectionRefusal::MissingLine => "Error: could not bind line digest".to_owned(),
        SelectionRefusal::UnsupportedEncryptFormat => {
            "Error: finding is not in an encrypt-supported format".to_owned()
        }
        SelectionRefusal::ResourceLimit => "Error: remediation scan resource limit".to_owned(),
        SelectionRefusal::Cancelled => "Error: remediation scan cancelled".to_owned(),
        SelectionRefusal::DeadlineExceeded => {
            "Error: remediation scan deadline exceeded".to_owned()
        }
    }
}

/// Same honesty rule for externalize, encrypt, and dismiss: a rescan that
/// is still sensitive is not `ok`.
fn apply_status_for_rescan(rescan: &str) -> &'static str {
    if rescan.starts_with("still_sensitive") {
        "apply_status: incomplete (rescan still_sensitive)"
    } else {
        "apply_status: ok"
    }
}

/// Unix credential mode after chmod. File-type bits above `0o777` are ignored.
#[cfg(all(unix, test))]
fn reject_non_owner_env_mode(mode: u32) -> Result<(), String> {
    let mode = mode & 0o777;
    if mode == 0o600 {
        Ok(())
    } else {
        Err(format!(
            ".env mode is {mode:#o} after chmod; expected owner-only 0o600"
        ))
    }
}

fn history_note(root: &Path) -> String {
    match crate::git::head_sha(root) {
        Ok(sha) => format!("history_note: old value may remain in git history since commit {sha}"),
        Err(_) => "history_note: working tree / uncommitted — no commit SHA observed".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::apply_status_for_rescan;

    #[test]
    fn still_sensitive_rescan_is_incomplete() {
        assert_eq!(
            apply_status_for_rescan("still_sensitive finding_count=1"),
            "apply_status: incomplete (rescan still_sensitive)"
        );
        assert_eq!(apply_status_for_rescan("clean"), "apply_status: ok");
        assert_eq!(
            apply_status_for_rescan("indeterminate TooLarge"),
            "apply_status: ok"
        );
        assert_eq!(
            apply_status_for_rescan("unreadable after write: boom"),
            "apply_status: ok"
        );
    }

    #[cfg(unix)]
    #[test]
    fn non_owner_env_mode_is_a_failure() {
        use super::reject_non_owner_env_mode;
        let err = reject_non_owner_env_mode(0o644).unwrap_err();
        assert!(err.contains("0o644"), "{err}");
        assert!(reject_non_owner_env_mode(0o666).is_err());
        assert!(reject_non_owner_env_mode(0o600).is_ok());
        // `Permissions::mode` includes the file type above the permission bits.
        assert!(reject_non_owner_env_mode(0o100600).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn chmod_failure_on_missing_env_is_surfaced() {
        use super::ensure_env_owner_only;
        let missing =
            std::env::temp_dir().join(format!("symforge-missing-env-{}", std::process::id()));
        let _ = std::fs::remove_file(&missing);
        let err = ensure_env_owner_only(&missing).unwrap_err();
        assert!(err.contains("chmod .env 0o600 failed"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn owner_only_chmod_lands_0600() {
        use super::ensure_env_owner_only;
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".env");
        std::fs::write(&path, b"K=v\n").unwrap();
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o644);
        std::fs::set_permissions(&path, perms).unwrap();
        ensure_env_owner_only(&path).unwrap();
        let mode = std::fs::symlink_metadata(&path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }
}
