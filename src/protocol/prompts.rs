use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{GetPromptResult, PromptMessage, Role};
use rmcp::{prompt, prompt_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::SymForgeServer;
use crate::protocol::resources::{
    file_context_resource, repo_changes_resource, repo_health_resource, repo_map_resource,
    repo_outline_resource, tools_catalog_resource,
};

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct CodeReviewPromptInput {
    pub path: Option<String>,
    pub focus: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct ArchitectureMapPromptInput {
    pub area: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct FailureTriagePromptInput {
    pub symptom: String,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct OnboardPromptInput {
    /// Optional area or module to focus on first.
    pub area: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct RefactorPromptInput {
    /// What you want to refactor (e.g., "rename ErrorKind to AppError", "extract validation logic").
    pub goal: String,
    /// Optional file or symbol to start from.
    pub target: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct DebugPromptInput {
    /// The error message, stack trace, or unexpected behavior.
    pub error: String,
    /// Optional file path where the error occurs.
    pub path: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct AdminPromptInput {}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct KnowledgeHygienePromptInput {
    /// Optional repository-relative path prefix to constrain the review.
    pub path_prefix: Option<String>,
}

#[prompt_router(vis = "pub(crate)")]
impl SymForgeServer {
    #[prompt(
        name = "symforge-review",
        description = "Generate a code review plan using SymForge context surfaces."
    )]
    pub(crate) async fn code_review_prompt(
        &self,
        params: Parameters<CodeReviewPromptInput>,
    ) -> GetPromptResult {
        let mut messages = vec![
            PromptMessage::new_text(
                Role::User,
                build_code_review_instructions(&self.project_name, &params.0),
            ),
            PromptMessage::new_resource_link(Role::User, repo_health_resource()),
            PromptMessage::new_resource_link(Role::User, repo_map_resource()),
        ];

        if let Some(path) = params.0.path.as_deref() {
            messages.push(PromptMessage::new_resource_link(
                Role::User,
                file_context_resource(path, Some(200)),
            ));
        }

        GetPromptResult::new(messages)
            .with_description("Review code using SymForge resources and targeted tools.")
    }

    #[prompt(
        name = "symforge-architecture",
        description = "Generate an architecture mapping plan using SymForge repo context."
    )]
    pub(crate) async fn architecture_map_prompt(
        &self,
        params: Parameters<ArchitectureMapPromptInput>,
    ) -> GetPromptResult {
        let mut messages = vec![
            PromptMessage::new_text(
                Role::User,
                build_architecture_map_instructions(&self.project_name, params.0.area.as_deref()),
            ),
            PromptMessage::new_resource_link(Role::User, repo_map_resource()),
            PromptMessage::new_resource_link(Role::User, repo_outline_resource()),
            PromptMessage::new_resource_link(Role::User, repo_health_resource()),
        ];

        if let Some(area) = params.0.area.as_deref() {
            messages.push(PromptMessage::new_text(
                Role::User,
                format!("Prioritize the area or subsystem named '{area}' if it exists."),
            ));
        }

        GetPromptResult::new(messages).with_description(
            "Map repository architecture using SymForge resources and cross-reference tools.",
        )
    }

    #[prompt(
        name = "symforge-triage",
        description = "Generate a debugging and failure-triage plan using SymForge state."
    )]
    pub(crate) async fn failure_triage_prompt(
        &self,
        params: Parameters<FailureTriagePromptInput>,
    ) -> GetPromptResult {
        let mut messages = vec![
            PromptMessage::new_text(
                Role::User,
                build_failure_triage_instructions(&self.project_name, &params.0),
            ),
            PromptMessage::new_resource_link(Role::User, repo_health_resource()),
            PromptMessage::new_resource_link(Role::User, repo_changes_resource()),
            PromptMessage::new_resource_link(Role::User, repo_map_resource()),
        ];

        if let Some(path) = params.0.path.as_deref() {
            messages.push(PromptMessage::new_resource_link(
                Role::User,
                file_context_resource(path, Some(200)),
            ));
        }

        GetPromptResult::new(messages).with_description(
            "Triage failures using SymForge runtime health, changed files, and local context.",
        )
    }

    #[prompt(
        name = "symforge-onboard",
        description = "Generate a codebase onboarding plan using SymForge for guided exploration."
    )]
    pub(crate) async fn onboard_prompt(
        &self,
        params: Parameters<OnboardPromptInput>,
    ) -> GetPromptResult {
        let mut messages = vec![
            PromptMessage::new_text(
                Role::User,
                build_onboard_instructions(&self.project_name, params.0.area.as_deref()),
            ),
            PromptMessage::new_resource_link(Role::User, repo_map_resource()),
            PromptMessage::new_resource_link(Role::User, repo_outline_resource()),
            PromptMessage::new_resource_link(Role::User, repo_health_resource()),
            PromptMessage::new_resource_link(Role::User, tools_catalog_resource()),
        ];

        if let Some(area) = params.0.area.as_deref() {
            messages.push(PromptMessage::new_text(
                Role::User,
                format!("Focus onboarding on the '{area}' area first."),
            ));
        }

        GetPromptResult::new(messages)
            .with_description("Onboard to a codebase using layered SymForge exploration.")
    }

    #[prompt(
        name = "symforge-refactor",
        description = "Generate a refactoring plan with impact analysis using SymForge."
    )]
    pub(crate) async fn refactor_prompt(
        &self,
        params: Parameters<RefactorPromptInput>,
    ) -> GetPromptResult {
        let mut messages = vec![
            PromptMessage::new_text(
                Role::User,
                build_refactor_instructions(&self.project_name, &params.0),
            ),
            PromptMessage::new_resource_link(Role::User, repo_map_resource()),
            PromptMessage::new_resource_link(Role::User, repo_health_resource()),
        ];

        if let Some(target) = params.0.target.as_deref() {
            messages.push(PromptMessage::new_resource_link(
                Role::User,
                file_context_resource(target, Some(200)),
            ));
        }

        GetPromptResult::new(messages)
            .with_description("Plan a refactoring with full impact analysis using SymForge.")
    }

    #[prompt(
        name = "symforge-debug",
        description = "Generate a detailed debugging plan using SymForge call tracing and change analysis."
    )]
    pub(crate) async fn debug_prompt(
        &self,
        params: Parameters<DebugPromptInput>,
    ) -> GetPromptResult {
        let mut messages = vec![
            PromptMessage::new_text(
                Role::User,
                build_debug_instructions(&self.project_name, &params.0),
            ),
            PromptMessage::new_resource_link(Role::User, repo_health_resource()),
            PromptMessage::new_resource_link(Role::User, repo_changes_resource()),
        ];

        if let Some(path) = params.0.path.as_deref() {
            messages.push(PromptMessage::new_resource_link(
                Role::User,
                file_context_resource(path, Some(200)),
            ));
        }

        GetPromptResult::new(messages)
            .with_description("Debug a problem using SymForge call tracing and impact analysis.")
    }

    #[prompt(
        name = "symforge-knowledge-hygiene",
        description = "Review repository knowledge evidence and prepare advisory remediation proposals without mutating files."
    )]
    pub(crate) async fn knowledge_hygiene_prompt(
        &self,
        params: Parameters<KnowledgeHygienePromptInput>,
    ) -> GetPromptResult {
        let messages = vec![
            PromptMessage::new_text(
                Role::User,
                build_knowledge_hygiene_instructions(
                    &self.project_name,
                    params.0.path_prefix.as_deref(),
                ),
            ),
            PromptMessage::new_resource_link(Role::User, repo_health_resource()),
            PromptMessage::new_resource_link(Role::User, repo_map_resource()),
        ];
        GetPromptResult::new(messages).with_description(
            "Review exact knowledge evidence, propose the smallest safe remediation, and stop for explicit approval before any preview or mutation.",
        )
    }

    #[prompt(
        name = "symforge-admin",
        description = "Return the running SymForge operator dashboard URL, or guidance to start it."
    )]
    pub(crate) async fn admin_prompt(
        &self,
        _params: Parameters<AdminPromptInput>,
    ) -> GetPromptResult {
        // The universal in-harness affordance (FR-016, Constitution II): every
        // MCP-speaking harness gets this, with or without a command file.
        //
        // Execution-model decision (009 US3): a prompt handler is a pure,
        // read-only fetch. It does NOT start a server — starting one is a
        // blocking, side-effectful process action that belongs to the
        // `symforge admin` CLI verb, and `operator_server_reachable` /
        // `start_operator_server` build their own current-thread tokio runtime and
        // `block_on`, which would panic if invoked directly on this async worker.
        // Instead the prompt does the read-only reachability check off the async
        // runtime (via `spawn_blocking`) and returns the reachable dashboard URL,
        // or guidance to run `symforge admin` when nothing is up yet.
        let repo_root = self.capture_repo_root();
        let dashboard_url =
            tokio::task::spawn_blocking(move || resolve_running_dashboard_url(repo_root))
                .await
                .unwrap_or(None);

        let body = build_admin_instructions(&self.project_name, dashboard_url.as_deref());
        GetPromptResult::new(vec![PromptMessage::new_text(Role::User, body)])
            .with_description(
                "Open the SymForge operator dashboard (reuse the running server, or start it via `symforge admin`).",
            )
    }
}

fn build_knowledge_hygiene_instructions(project_name: &str, path_prefix: Option<&str>) -> String {
    crate::prompt_engine::build_knowledge_hygiene_instructions(project_name, path_prefix)
}

/// Read-only resolution of the running operator dashboard URL for the
/// `symforge-admin` prompt: load the project's `OperatorSetupProfile` for the
/// remembered port and, if a server is reachable there, return its `/admin` URL.
/// Returns `None` when there is no profile or nothing serves the remembered port.
///
/// This performs an HTTP reachability probe on its own current-thread runtime, so
/// it MUST run off the async worker (the caller wraps it in `spawn_blocking`). It
/// never starts a server — that is the `symforge admin` CLI verb's job (D3).
fn resolve_running_dashboard_url(repo_root: Option<std::path::PathBuf>) -> Option<String> {
    let control_state_dir = crate::paths::process_control_state_placement().directory()?;
    resolve_running_dashboard_url_in(repo_root, control_state_dir)
}

/// Reachability check against an EXPLICIT control-state dir.
///
/// `resolve_running_dashboard_url` reached straight for the process-global
/// placement, which is a `OnceLock` and therefore not overridable. That left the
/// no-server guidance test unable to isolate itself: any sibling test that
/// started a real operator server and wrote a profile made this probe succeed,
/// so the prompt correctly reported a running dashboard and the test failed by
/// execution order. Taking the dir as an argument makes the dependency
/// injectable without changing which dir production resolves.
fn resolve_running_dashboard_url_in(
    _repo_root: Option<std::path::PathBuf>,
    control_state_dir: &crate::domain::ControlStateDir,
) -> Option<String> {
    use std::time::Duration;
    let profile = crate::cli::operator_profile::OperatorSetupProfile::load(control_state_dir)?;
    let addr = std::net::SocketAddr::new(
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        profile.port,
    );
    if crate::cli::admin::operator_server_reachable(addr, Duration::from_millis(500)) {
        Some(format!("http://{addr}/admin"))
    } else {
        None
    }
}

fn build_admin_instructions(project_name: &str, dashboard_url: Option<&str>) -> String {
    crate::prompt_engine::build_admin_instructions(project_name, dashboard_url)
}

/// Shared surface-mapping preamble for the six tool-driven prompt bodies. The
/// step tool names below are the full-surface spellings, directly callable on
/// the default full surface. On the opt-in compact surface
/// (`SYMFORGE_SURFACE=compact`) the agent has only
/// `symforge`/`symforge_edit`/`status`, so the mapping is stated once here
/// instead of dual-spelling every step. `build_admin_instructions` is excluded:
/// it drives the `symforge admin` CLI verb, not the MCP tool surface.

fn build_code_review_instructions(project_name: &str, input: &CodeReviewPromptInput) -> String {
    crate::prompt_engine::build_code_review_instructions(
        project_name,
        input.path.as_deref(),
        input.focus.as_deref(),
    )
}

fn build_architecture_map_instructions(project_name: &str, area: Option<&str>) -> String {
    crate::prompt_engine::build_architecture_map_instructions(project_name, area)
}

fn build_failure_triage_instructions(
    project_name: &str,
    input: &FailureTriagePromptInput,
) -> String {
    crate::prompt_engine::build_failure_triage_instructions(
        project_name,
        &input.symptom,
        input.path.as_deref(),
    )
}

fn build_onboard_instructions(project_name: &str, area: Option<&str>) -> String {
    crate::prompt_engine::build_onboard_instructions(project_name, area)
}

fn build_refactor_instructions(project_name: &str, input: &RefactorPromptInput) -> String {
    crate::prompt_engine::build_refactor_instructions(
        project_name,
        &input.goal,
        input.target.as_deref(),
    )
}

fn build_debug_instructions(project_name: &str, input: &DebugPromptInput) -> String {
    crate::prompt_engine::build_debug_instructions(
        project_name,
        &input.error,
        input.path.as_deref(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use crate::live_index::store::{CircuitBreakerState, LiveIndex};
    use crate::watcher::WatcherInfo;

    use crate::protocol::resources::REPO_HEALTH_URI;

    fn make_server() -> SymForgeServer {
        let index = LiveIndex {
            files: HashMap::new(),
            loaded_at: Instant::now(),
            loaded_at_system: std::time::SystemTime::now(),
            load_duration: Duration::from_millis(1),
            cb_state: CircuitBreakerState::new(0.20),
            is_empty: false,
            load_source: crate::live_index::store::IndexLoadSource::FreshLoad,
            snapshot_verify_state: crate::live_index::store::SnapshotVerifyState::NotNeeded,
            reverse_index: HashMap::new(),
            files_by_basename: HashMap::new(),
            files_by_dir_component: HashMap::new(),
            trigram_index: crate::live_index::trigram::TrigramIndex::new(),
            gitignore: None,
            manifest_entries: Vec::new(),
            coupling_store: None,
            local_empty_reason: std::sync::Arc::new(parking_lot::RwLock::new(None)),
            indexed_root: None,
        };

        SymForgeServer::new(
            crate::live_index::SharedIndexHandle::shared(index),
            "prompt_project".to_string(),
            Arc::new(Mutex::new(WatcherInfo::default())),
            None,
            None,
        )
    }

    #[test]
    fn test_prompt_router_lists_expected_prompts() {
        let server = make_server();
        let prompts = server.prompt_router.list_all();
        let by_name: HashMap<&str, &rmcp::model::Prompt> =
            prompts.iter().map(|p| (p.name.as_str(), p)).collect();

        // All eight prompts are advertised. 009 US3 (T024): symforge-admin is the
        // universal in-harness affordance.
        for name in [
            "symforge-review",
            "symforge-architecture",
            "symforge-triage",
            "symforge-onboard",
            "symforge-refactor",
            "symforge-debug",
            "symforge-knowledge-hygiene",
            "symforge-admin",
        ] {
            assert!(by_name.contains_key(name), "missing prompt: {name}");
        }

        // Regression guard for the prompts/list `description` each client renders
        // in its slash-command menu. A client once showed `symforge-architecture`
        // mangled ("using Synts: area)"); the SOURCE string was intact, so the
        // mangling was client-side truncation. This pins every source description
        // exactly, so a genuine source-side corruption would fail here.
        let expect_desc = |name: &str, desc: &str| {
            assert_eq!(
                by_name[name].description.as_deref(),
                Some(desc),
                "list description drift for {name}"
            );
        };
        expect_desc(
            "symforge-review",
            "Generate a code review plan using SymForge context surfaces.",
        );
        expect_desc(
            "symforge-architecture",
            "Generate an architecture mapping plan using SymForge repo context.",
        );
        expect_desc(
            "symforge-triage",
            "Generate a debugging and failure-triage plan using SymForge state.",
        );
        expect_desc(
            "symforge-onboard",
            "Generate a codebase onboarding plan using SymForge for guided exploration.",
        );
        expect_desc(
            "symforge-refactor",
            "Generate a refactoring plan with impact analysis using SymForge.",
        );
        expect_desc(
            "symforge-debug",
            "Generate a detailed debugging plan using SymForge call tracing and change analysis.",
        );
        expect_desc(
            "symforge-knowledge-hygiene",
            "Review repository knowledge evidence and prepare advisory remediation proposals without mutating files.",
        );
        expect_desc(
            "symforge-admin",
            "Return the running SymForge operator dashboard URL, or guidance to start it.",
        );

        // Declared arguments (prompts/list) must match the input structs,
        // including which are optional. rmcp derives `required` from the schema:
        // `Option<_>` fields are not required.
        let args = |name: &str| -> HashMap<String, bool> {
            by_name[name]
                .arguments
                .as_ref()
                .map(|list| {
                    list.iter()
                        .map(|a| (a.name.clone(), a.required.unwrap_or(false)))
                        .collect()
                })
                .unwrap_or_default()
        };
        assert_eq!(
            args("symforge-review"),
            HashMap::from([("path".into(), false), ("focus".into(), false)]),
        );
        assert_eq!(
            args("symforge-architecture"),
            HashMap::from([("area".into(), false)]),
        );
        assert_eq!(
            args("symforge-triage"),
            HashMap::from([("symptom".into(), true), ("path".into(), false)]),
        );
        assert_eq!(
            args("symforge-onboard"),
            HashMap::from([("area".into(), false)]),
        );
        assert_eq!(
            args("symforge-refactor"),
            HashMap::from([("goal".into(), true), ("target".into(), false)]),
        );
        assert_eq!(
            args("symforge-debug"),
            HashMap::from([("error".into(), true), ("path".into(), false)]),
        );
        assert_eq!(
            args("symforge-knowledge-hygiene"),
            HashMap::from([("path_prefix".into(), false)]),
        );
        // AdminPromptInput has no fields -> no declared arguments.
        assert!(args("symforge-admin").is_empty());
    }

    #[test]
    fn knowledge_hygiene_prompt_pins_advisory_flow_and_rejects_sensitive_scope_without_echo() {
        let instructions =
            build_knowledge_hygiene_instructions("prompt_project", Some("docs/design"));
        for required in [
            "review_knowledge(mode=\"summary\"",
            "review_knowledge(mode=\"remediation\")",
            "mode=\"document\"",
            "STOP for explicit user approval",
            "curate_knowledge preview",
            "prompt cannot approve",
            "never moves, edits, or deletes",
        ] {
            assert!(
                instructions.contains(required),
                "missing hygiene workflow contract {required}: {instructions}"
            );
        }

        let canary = ["runtime", "-", "prompt", "-", "canary"].concat();
        let sensitive_scope = format!("docs/token={canary}");
        let rejected =
            build_knowledge_hygiene_instructions("prompt_project", Some(&sensitive_scope));
        assert!(rejected.contains("rejected by repository safety policy"));
        assert!(!rejected.contains(&canary), "sensitive scope must not echo");
        assert!(
            !rejected.contains("review_knowledge") && !rejected.contains("curate_knowledge"),
            "a rejected prompt must not create a review or proposal workflow"
        );
    }

    #[test]
    fn resolve_running_dashboard_url_in_is_none_without_a_profile() {
        // Isolation contract: resolution reads the profile from the control-state
        // dir it is GIVEN. Previously it reached for the process-global
        // placement (a OnceLock), so a sibling test that started a real operator
        // server made this path resolve a URL and the no-server guidance test
        // below failed purely by execution order.
        let dir = tempfile::tempdir().expect("temp control state");
        let control = crate::domain::ControlStateDir::new(dir.path().join("control"));
        assert_eq!(
            resolve_running_dashboard_url_in(None, &control),
            None,
            "an empty control-state dir has no profile, so no dashboard resolves"
        );
    }

    #[test]
    fn test_admin_prompt_returns_guidance_when_no_server() {
        // No profile in this isolated control-state dir, so nothing resolves: the
        // prompt body must be the honest "run `symforge admin`" guidance rather
        // than a fabricated URL (URL-or-guidance, D2).
        let dir = tempfile::tempdir().expect("temp control state");
        let control = crate::domain::ControlStateDir::new(dir.path().join("control"));
        let dashboard_url = resolve_running_dashboard_url_in(None, &control);
        assert_eq!(dashboard_url, None, "no profile must resolve no dashboard");
        let text = build_admin_instructions("prompt_project", dashboard_url.as_deref());

        // Guidance points at the CLI verb that actually starts the server.
        assert!(
            text.contains("symforge admin"),
            "no-server guidance must name the `symforge admin` verb: {text}"
        );
        assert!(
            text.contains("No operator dashboard is currently running"),
            "must honestly report nothing is running: {text}"
        );
        // D21: a fresh `symforge admin` stays in the foreground (does not fire-and-
        // return); the guidance must describe the foreground-serving behavior, while
        // the reuse path returns immediately.
        assert!(
            text.contains("stays in the foreground"),
            "guidance must describe foreground serving on fresh start (D21): {text}"
        );
        assert!(
            text.contains("returns immediately"),
            "guidance must state the reuse path returns immediately: {text}"
        );
    }

    #[tokio::test]
    async fn test_admin_prompt_returns_a_text_message() {
        // End-to-end wiring: the handler resolves off the async runtime and always
        // returns a body. Whether THIS machine has an operator server running is
        // ambient state, so assert only what holds either way — the URL-vs-guidance
        // split is covered against an isolated control-state dir above.
        let server = make_server();
        let result = server
            .admin_prompt(Parameters(AdminPromptInput::default()))
            .await;

        let text = result
            .messages
            .iter()
            .find_map(|message| match &message.content {
                rmcp::model::ContentBlock::Text(content) => Some(content.text.clone()),
                _ => None,
            })
            .expect("admin prompt returns a text message");
        assert!(
            text.contains("SymForge Operator Dashboard"),
            "every admin prompt body carries the dashboard heading: {text}"
        );
    }

    #[test]
    fn test_admin_instructions_report_running_url_when_present() {
        // When a dashboard URL is resolved, the prompt body surfaces exactly that
        // URL (the reuse path) and does not fall back to guidance.
        let url = "http://127.0.0.1:8787/admin";
        let body = build_admin_instructions("prompt_project", Some(url));
        assert!(
            body.contains(url),
            "running body must include the URL: {body}"
        );
        assert!(
            body.contains("dashboard is running"),
            "running body must say the dashboard is up: {body}"
        );
    }

    #[tokio::test]
    async fn test_code_review_prompt_includes_resource_links() {
        let server = make_server();
        let result = server
            .code_review_prompt(Parameters(CodeReviewPromptInput {
                path: Some("src/lib.rs".to_string()),
                focus: Some("dependency risks".to_string()),
            }))
            .await;

        assert!(
            result.messages.iter().any(|message| matches!(
                &message.content,
                rmcp::model::ContentBlock::ResourceLink(link)
                    if link.uri == REPO_HEALTH_URI
            )),
            "symforge-review prompt should link repo health"
        );
        assert!(
            result.messages.iter().any(|message| matches!(
                &message.content,
                rmcp::model::ContentBlock::ResourceLink(link)
                    if link.uri.contains("symforge://file/context")
            )),
            "symforge-review prompt should link file context"
        );
    }

    #[test]
    fn test_onboard_instructions_embed_orientation_doctrine() {
        let body = build_onboard_instructions("prompt_project", None);
        // Statement 1: the map orients; the tools prove.
        assert!(
            body.contains("map orients"),
            "onboarding instructions must embed the 'map orients' doctrine: {body}"
        );
        // Statement 2: absence from the map is not absence from the repo.
        assert!(
            body.contains("not absence from the repo"),
            "onboarding instructions must embed the 'not absence' doctrine: {body}"
        );
        assert!(
            body.contains("search_symbols") && body.contains("search_text"),
            "onboarding doctrine must point at search_symbols / search_text: {body}"
        );
    }

    #[test]
    fn test_architecture_map_instructions_embed_orientation_doctrine() {
        let body = build_architecture_map_instructions("prompt_project", None);
        // Statement 1: the map orients; the tools prove.
        assert!(
            body.contains("map orients"),
            "architecture instructions must embed the 'map orients' doctrine: {body}"
        );
        // Statement 2: absence from the map is not absence from the repo.
        assert!(
            body.contains("not absence from the repo"),
            "architecture instructions must embed the 'not absence' doctrine: {body}"
        );
        assert!(
            body.contains("search_symbols") && body.contains("search_text"),
            "architecture doctrine must point at search_symbols / search_text: {body}"
        );
    }

    #[test]
    fn test_tool_driven_instructions_are_surface_aware_and_current() {
        let review = build_code_review_instructions(
            "p",
            &CodeReviewPromptInput {
                path: None,
                focus: None,
            },
        );
        let architecture = build_architecture_map_instructions("p", None);
        let triage = build_failure_triage_instructions(
            "p",
            &FailureTriagePromptInput {
                symptom: "boom".to_string(),
                path: None,
            },
        );
        let onboard = build_onboard_instructions("p", None);
        let refactor = build_refactor_instructions(
            "p",
            &RefactorPromptInput {
                goal: "extract validation".to_string(),
                target: None,
            },
        );
        let debug = build_debug_instructions(
            "p",
            &DebugPromptInput {
                error: "panic".to_string(),
                path: None,
            },
        );

        // Every tool-driven body carries the single upfront surface-mapping note so a
        // compact-surface agent knows the step tool names are full-surface spellings
        // and how to route them through `symforge`/`symforge_edit`.
        for (label, body) in [
            ("review", &review),
            ("architecture", &architecture),
            ("triage", &triage),
            ("onboard", &onboard),
            ("refactor", &refactor),
            ("debug", &debug),
        ] {
            assert!(
                body.contains("Surface note:") && body.contains("`symforge_edit`"),
                "{label} body must embed the surface-mapping note: {body}"
            );
        }

        // Current-reality capability claims land only where they help the plan.
        assert!(
            review.contains("detect_impact") && review.contains("caps at 200"),
            "review must mention detect_impact + the what_changed 200 cap: {review}"
        );
        assert!(
            triage.contains("detect_impact") && triage.contains("caps at 200"),
            "triage must mention detect_impact + the what_changed 200 cap: {triage}"
        );
        assert!(
            refactor.contains("detect_impact"),
            "refactor verify step must mention detect_impact: {refactor}"
        );
        assert!(
            debug.contains("get_symbol(name=")
                && debug.contains("detect_impact")
                && debug.contains("caps at 200"),
            "debug must use name-only get_symbol + detect_impact + the 200 cap: {debug}"
        );
    }
}
