//! MCP `SYMFORGE_SURFACE=code` tool router (`code_search` and `code_execute`).
//!
//! Exposes the pure-Rust `monty` Code Mode sandbox over MCP for harnesses without
//! direct shell access (or operators running `symforge serve --surface code`).
//! Both tools are filtered out of `SurfaceProfile::Full` (leaving Full at exact 40)
//! and advertised on `SurfaceProfile::Code` alongside `symforge_edit` and `status`.

use std::path::PathBuf;
use std::time::Duration;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::SymForgeServer;
use super::result_status::{OutcomeClass, ResultStatus};
use crate::codemode::{
    CodeModeLimits, DaemonCodeBackend, ExecuteRequest, ExecuteStatus, LocalCodeBackend,
    execute_script, render_stubs,
};

pub const CODE_OUTPUT_ENV: &str = "SYMFORGE_CODE_OUTPUT";
pub const CODE_EXECUTE_META_KEY: &str = "symforge/code_execute";

/// Input for `code_search` on `SYMFORGE_SURFACE=code`.
#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct CodeSearchInput {
    /// Topic or function name to inspect (omit for 1-line index of all 15 read functions).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    /// Max matching stubs to return when `query` is set (default 5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

/// Input for `code_execute` on `SYMFORGE_SURFACE=code`.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct CodeExecuteInput {
    /// Python 3.14-subset script calling keyword-only host read functions; final expression is returned as lean text.
    pub code: String,
    /// Project ID/alias (not a path); omit for the active or only project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// Optional interpreter feed budget in milliseconds (default 10000, max 30000).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

#[tool_router(router = code_mode_tool_router, vis = "pub(crate)")]
impl SymForgeServer {
    #[tool(
        name = "code_search",
        description = "Discover Python read functions and TypedDict return types for code_execute. Omit query for 1-line index.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    pub(crate) async fn code_search_tool(
        &self,
        params: Parameters<CodeSearchInput>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        let limit = params.0.limit.map(|n| n as usize);
        let text = render_stubs(params.0.query.as_deref(), limit);
        Ok(ResultStatus::new(OutcomeClass::Found).into_call_tool_result(text))
    }

    #[tool(
        name = "code_execute",
        description = "Run one zero-egress Python script chaining read-only index queries; returns the final value as lean text (tables, raw strings), followed by notices when results are capped or withheld.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    pub(crate) async fn code_execute_tool(
        &self,
        params: Parameters<CodeExecuteInput>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        let input = params.0;
        if input.code.trim().is_empty() {
            return Ok(ResultStatus::new(OutcomeClass::InvalidRequest)
                .into_call_tool_result("Error: `code` must be a non-empty Python script."));
        }

        let selector = input
            .project
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned);

        let mut limits = CodeModeLimits {
            allow_spill_files: matches!(self.transport_lane, super::repeat::TransportLane::Stdio),
            lean_output: !std::env::var(CODE_OUTPUT_ENV)
                .is_ok_and(|v| v.eq_ignore_ascii_case("json")),
            ..CodeModeLimits::default()
        };
        if let Some(ms) = input.timeout_ms {
            limits.max_feed_duration = Duration::from_millis(ms);
        }
        let exec_req = ExecuteRequest {
            code: input.code,
            limits: limits.clamped(),
            spill_dir: None,
        };

        // 1. Daemon-proxied server: mirror `proxy_tool_call` routing, evidence clearing,
        //    reconnect-once, and safe HOME fallback refusal when ACTIVE moved off HOME.
        if let Some(daemon_lock) = self.daemon_client.as_ref() {
            let was_degraded = self.daemon_degraded.lock().degraded;
            let (local_fallback_refusal, pinned_project_id) = {
                let client = daemon_lock.read().await;
                let has_topology =
                    client.project_root().is_some() || client.activated_root().is_some();
                match selector.as_deref() {
                    Some(sel) => match client.opened_project_for_selector(Some(sel)) {
                        Some((project_id, _, true)) => (None, Some(project_id)),
                        Some((project_id, _, false)) => (
                            Some(format!(
                                "Error: project_routing: daemon project '{project_id}' cannot \
                                 fall back to adapter HOME project '{}'. Restore daemon routing \
                                 and retry.",
                                client.project_id()
                            )),
                            Some(project_id),
                        ),
                        None => (
                            Some(format!(
                                "Error: project_routing: project selector '{sel}' cannot be resolved \
                                 for safe HOME fallback. Restore daemon routing and retry."
                            )),
                            Some(if has_topology {
                                "\0unresolved-project-selector".to_string()
                            } else {
                                sel.to_string()
                            }),
                        ),
                    },
                    None => {
                        let active_id = client.active_project_id();
                        (
                            (!client.active_project_is_home()).then(|| {
                                format!(
                                    "Error: project_routing: ACTIVE daemon project '{}' cannot fall \
                                     back to adapter HOME project '{}'. Restore daemon routing and \
                                     retry.",
                                    active_id,
                                    client.project_id()
                                )
                            }),
                            has_topology.then_some(active_id),
                        )
                    }
                }
            };

            // Clear stale local HOME evidence before crossing the daemon boundary.
            super::result_status::clear_project_evidence();

            let run_against_daemon = |session: crate::daemon::DaemonSessionClient,
                                      req: ExecuteRequest,
                                      proj: Option<String>| async move {
                let handle = tokio::runtime::Handle::current();
                tokio::task::spawn_blocking(move || {
                    let backend = DaemonCodeBackend::new_borrowed(handle, session, proj);
                    let outcome = execute_script(&req, &backend);
                    let evidence = backend.take_last_evidence();
                    (outcome, evidence)
                })
                .await
                .map_err(|join| {
                    rmcp::ErrorData::internal_error(
                        format!("code_execute worker panicked: {join}"),
                        None,
                    )
                })
            };

            let is_transport_failure =
                |outcome: &crate::codemode::ExecuteOutcome| -> Option<String> {
                    if outcome.status != ExecuteStatus::Error {
                        return None;
                    }
                    let msg = outcome.error.as_ref().map(|e| e.message.as_str())?;
                    msg.contains("daemon_transport:").then(|| msg.to_owned())
                };

            let first_session = daemon_lock.read().await.clone();
            let (first_outcome, first_evidence) = run_against_daemon(
                first_session.clone(),
                exec_req.clone(),
                pinned_project_id.clone(),
            )
            .await?;

            let Some(transport_err) = is_transport_failure(&first_outcome) else {
                if let Some(ev) = first_evidence
                    && super::result_status::project_evidence_matches_selector(
                        &ev,
                        selector.as_deref(),
                    )
                {
                    super::result_status::record_project_evidence(ev);
                }
                self.note_daemon_recovered();
                return render_execute_call_result(first_outcome);
            };

            self.note_daemon_failure(first_session.base_url(), transport_err);

            let cooling = was_degraded
                && self
                    .daemon_degraded
                    .lock()
                    .last_reconnect_attempt
                    .map(|attempt| attempt.elapsed() < super::DAEMON_RECONNECT_COOLDOWN)
                    .unwrap_or(false);
            if cooling {
                if let Some(refusal) = local_fallback_refusal {
                    super::result_status::clear_project_evidence();
                    return Ok(ResultStatus::new(OutcomeClass::InvalidRequest)
                        .into_call_tool_result(refusal));
                }
                self.ensure_local_index().await;
                self.restore_local_project_evidence();
            } else {
                let lane = daemon_lock.read().await.activation_lane();
                let _activation_guard = lane.lock_owned().await;
                let reconnected = {
                    let mut client = daemon_lock.write().await;
                    self.daemon_degraded.lock().last_reconnect_attempt =
                        Some(std::time::Instant::now());
                    match client.reconnect().await {
                        Ok(new_client) => {
                            let old_client = std::mem::replace(&mut *client, new_client);
                            self.repeat_tracker.lock().clear();
                            self.republish_session_descriptor(&client);
                            let _ = old_client.close().await;
                            self.note_daemon_recovered();
                            true
                        }
                        Err(reconnect_err) => {
                            self.note_daemon_failure(
                                client.base_url(),
                                format!("reconnect failed: {reconnect_err}"),
                            );
                            if local_fallback_refusal.is_none() {
                                self.ensure_local_index().await;
                            }
                            false
                        }
                    }
                };

                if reconnected {
                    let retry_session = daemon_lock.read().await.clone();
                    let (retry_outcome, retry_evidence) = run_against_daemon(
                        retry_session.clone(),
                        exec_req.clone(),
                        pinned_project_id,
                    )
                    .await?;
                    if let Some(retry_err) = is_transport_failure(&retry_outcome) {
                        self.note_daemon_failure(retry_session.base_url(), retry_err);
                        if let Some(refusal) = local_fallback_refusal {
                            super::result_status::clear_project_evidence();
                            return Ok(ResultStatus::new(OutcomeClass::InvalidRequest)
                                .into_call_tool_result(refusal));
                        }
                        self.ensure_local_index().await;
                        self.restore_local_project_evidence();
                    } else {
                        if let Some(ev) = retry_evidence
                            && super::result_status::project_evidence_matches_selector(
                                &ev,
                                selector.as_deref(),
                            )
                        {
                            super::result_status::record_project_evidence(ev);
                        }
                        self.note_daemon_recovered();
                        return render_execute_call_result(retry_outcome);
                    }
                } else {
                    if let Some(refusal) = local_fallback_refusal {
                        super::result_status::clear_project_evidence();
                        return Ok(ResultStatus::new(OutcomeClass::InvalidRequest)
                            .into_call_tool_result(refusal));
                    }
                    self.restore_local_project_evidence();
                }
            }
        }

        // 2. Local / serve / degraded-HOME-fallback server: validate foreign project selector.
        if let Some(refusal) = self.foreign_project_refusal(selector.as_deref()) {
            return Ok(
                ResultStatus::new(OutcomeClass::InvalidRequest).into_call_tool_result(refusal)
            );
        }

        let shared = self.index.shared();
        let repo_root: Option<PathBuf> = self.capture_repo_root();
        let project_name = self.project_name.clone();
        let project_state_dir = self.capture_project_state_dir();

        let outcome = tokio::task::spawn_blocking(move || {
            let backend = LocalCodeBackend::new(shared, repo_root, project_name, project_state_dir);
            execute_script(&exec_req, &backend)
        })
        .await
        .map_err(|join| {
            rmcp::ErrorData::internal_error(format!("code_execute worker panicked: {join}"), None)
        })?;

        render_execute_call_result(outcome)
    }
}

const WITHHELD_FILES_MEANING: &str = "files in this call's search scope that were not searched because they are withheld; they may or may not contain matches, and the number is not a count of files with more hits";

fn render_execute_call_result(
    mut outcome: crate::codemode::ExecuteOutcome,
) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
    let class = match outcome.status {
        ExecuteStatus::Ok => OutcomeClass::Found,
        ExecuteStatus::Error => {
            let msg = outcome
                .error
                .as_ref()
                .map(|e| e.message.as_str())
                .unwrap_or("");
            if msg.contains("not_found:") {
                OutcomeClass::NotFound
            } else if msg.contains("ambiguous_symbol:") {
                OutcomeClass::Ambiguous
            } else if msg.contains("index_loading:")
                || msg.contains("index_empty:")
                || msg.contains("index_degraded:")
            {
                OutcomeClass::InternalFailure
            } else {
                OutcomeClass::InvalidRequest
            }
        }
    };
    let text = if let Some(mut text) = outcome.lean_text.take() {
        if !outcome.notices.is_empty() {
            // Keep notices visible even in clients that display only the first text block.
            text.push_str("\n\nHost-call completeness: ");
            let mut suffix = serde_json::json!({"notices": outcome.notices});
            if outcome
                .notices
                .iter()
                .any(|n| n.completeness.withheld_files > 0)
            {
                suffix["withheld_files_meaning"] = serde_json::json!(WITHHELD_FILES_MEANING);
            }
            text.push_str(&suffix.to_string());
        }
        text
    } else {
        serde_json::to_string(&outcome)
            .unwrap_or_else(|e| format!("{{\"status\":\"error\",\"message\":\"{e}\"}}"))
    };
    let mut result = ResultStatus::new(class).into_call_tool_result(text);
    if let Some(meta) = result.meta.as_mut() {
        meta.0.insert(
            CODE_EXECUTE_META_KEY.to_string(),
            serde_json::json!({
                "calls": outcome.calls,
                "duration_ms": outcome.duration_ms,
                "output_bytes": outcome.output_bytes,
            }),
        );
    }
    Ok(result)
}
