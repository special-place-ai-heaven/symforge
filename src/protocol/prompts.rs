use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{GetPromptResult, PromptMessage, PromptMessageRole};
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
                PromptMessageRole::User,
                build_code_review_instructions(&self.project_name, &params.0),
            ),
            PromptMessage::new_resource_link(PromptMessageRole::User, repo_health_resource()),
            PromptMessage::new_resource_link(PromptMessageRole::User, repo_map_resource()),
        ];

        if let Some(path) = params.0.path.as_deref() {
            messages.push(PromptMessage::new_resource_link(
                PromptMessageRole::User,
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
                PromptMessageRole::User,
                build_architecture_map_instructions(&self.project_name, params.0.area.as_deref()),
            ),
            PromptMessage::new_resource_link(PromptMessageRole::User, repo_map_resource()),
            PromptMessage::new_resource_link(PromptMessageRole::User, repo_outline_resource()),
            PromptMessage::new_resource_link(PromptMessageRole::User, repo_health_resource()),
        ];

        if let Some(area) = params.0.area.as_deref() {
            messages.push(PromptMessage::new_text(
                PromptMessageRole::User,
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
                PromptMessageRole::User,
                build_failure_triage_instructions(&self.project_name, &params.0),
            ),
            PromptMessage::new_resource_link(PromptMessageRole::User, repo_health_resource()),
            PromptMessage::new_resource_link(PromptMessageRole::User, repo_changes_resource()),
            PromptMessage::new_resource_link(PromptMessageRole::User, repo_map_resource()),
        ];

        if let Some(path) = params.0.path.as_deref() {
            messages.push(PromptMessage::new_resource_link(
                PromptMessageRole::User,
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
                PromptMessageRole::User,
                build_onboard_instructions(&self.project_name, params.0.area.as_deref()),
            ),
            PromptMessage::new_resource_link(PromptMessageRole::User, repo_map_resource()),
            PromptMessage::new_resource_link(PromptMessageRole::User, repo_outline_resource()),
            PromptMessage::new_resource_link(PromptMessageRole::User, repo_health_resource()),
            PromptMessage::new_resource_link(PromptMessageRole::User, tools_catalog_resource()),
        ];

        if let Some(area) = params.0.area.as_deref() {
            messages.push(PromptMessage::new_text(
                PromptMessageRole::User,
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
                PromptMessageRole::User,
                build_refactor_instructions(&self.project_name, &params.0),
            ),
            PromptMessage::new_resource_link(PromptMessageRole::User, repo_map_resource()),
            PromptMessage::new_resource_link(PromptMessageRole::User, repo_health_resource()),
        ];

        if let Some(target) = params.0.target.as_deref() {
            messages.push(PromptMessage::new_resource_link(
                PromptMessageRole::User,
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
                PromptMessageRole::User,
                build_debug_instructions(&self.project_name, &params.0),
            ),
            PromptMessage::new_resource_link(PromptMessageRole::User, repo_health_resource()),
            PromptMessage::new_resource_link(PromptMessageRole::User, repo_changes_resource()),
        ];

        if let Some(path) = params.0.path.as_deref() {
            messages.push(PromptMessage::new_resource_link(
                PromptMessageRole::User,
                file_context_resource(path, Some(200)),
            ));
        }

        GetPromptResult::new(messages)
            .with_description("Debug a problem using SymForge call tracing and impact analysis.")
    }
}

fn build_code_review_instructions(project_name: &str, input: &CodeReviewPromptInput) -> String {
    let target = input.path.as_deref().map_or(
        "Start from what_changed(uncommitted=true, code_only=true) to find all modified files."
            .to_string(),
        |p| format!("Start with the target path '{p}'."),
    );
    let focus = input.focus.as_deref().map_or(String::new(), |f| {
        format!("\n\nPay special attention to: {f}.")
    });

    format!(
        "## Code Review Workflow for '{project_name}'\n\
         \n\
         ### Step 1: Scope the Review\n\
         {target}\n\
         - Call `diff_symbols(code_only=true)` to see which symbols changed\n\
         - If > 20 symbols changed, use `diff_symbols(compact=true)` first for overview\n\
         \n\
         ### Step 2: Prioritize by Risk\n\
         For each changed symbol:\n\
         - Call `find_references(name=\"<sym>\", compact=true)` — symbols with >5 callers are HIGH RISK\n\
         - Call `get_symbol_context(name=\"<sym>\", verbosity=\"signature\")` — check for signature changes\n\
         \n\
         ### Step 3: Deep Review (high-risk symbols only)\n\
         - Call `get_symbol_context(name=\"<sym>\", bundle=true)` to see the symbol + all type deps\n\
         - Check: Are all callers still compatible with the change?\n\
         - Check: Did any type dependency change shape?\n\
         - Check: Are there missing error-handling paths?\n\
         \n\
         ### Step 4: Check for Missing Tests\n\
         - For each modified symbol, search for corresponding test functions\n\
         - Call `search_symbols(query=\"test_<sym_name>\", include_tests=true)`\n\
         - Flag any changed symbols without test coverage\n\
         \n\
         ### Step 5: Report\n\
         Summarize: what changed, risk assessment per symbol, broken contracts, missing tests.{focus}"
    )
}

fn build_architecture_map_instructions(project_name: &str, area: Option<&str>) -> String {
    let area_note = area.map_or(String::new(), |a| {
        format!("\n\nFocus area: '{a}'. Prioritize this subsystem and its connections.")
    });

    format!(
        "## Architecture Mapping Workflow for '{project_name}'\n\
         \n\
         ### Step 1: Get the Big Picture\n\
         - Read the repo map resource (attached) for directory structure and key types\n\
         - Doctrine: the map orients; the tools prove. Treat the repo map as a ranked starting point, not an inventory.\n\
         - Absence from the map is not absence from the repo - confirm with `search_symbols` / `search_text` before concluding something is missing.\n\
         - Call `get_repo_map(detail=\"tree\", depth=2)` to see the file tree with symbol counts\n\
         - Identify the top-level modules/crates/packages\n\
         \n\
         ### Step 2: Map Subsystem Boundaries\n\
         For each major directory:\n\
         - Call `get_file_context(path=\"<dir>/mod.rs\", sections=[\"outline\",\"imports\"])` (or equivalent entry file)\n\
         - Note: which modules import which? What are the public exports?\n\
         - Call `find_dependents(path=\"<key_file>\", compact=true)` for core files\n\
         \n\
         ### Step 3: Identify the Core Types\n\
         - Call `search_symbols(kind=\"struct\", limit=20)` to find the main data structures\n\
         - For each core struct: `get_symbol_context(name=\"<struct>\", verbosity=\"signature\")` to see its shape\n\
         - Call `find_references(name=\"<struct>\", mode=\"implementations\")` to find trait impls\n\
         \n\
         ### Step 4: Trace Data Flow\n\
         - Pick 2-3 key entry points (main, handlers, API routes)\n\
         - Call `get_symbol_context(name=\"<entry>\", sections=[])` for full trace: callers → callees → types\n\
         - Follow the call chain 2-3 levels deep to map the hot path\n\
         \n\
         ### Step 5: Report\n\
         Produce: subsystem diagram, ownership boundaries, key data flows, and which files/symbols to investigate next.{area_note}"
    )
}

fn build_failure_triage_instructions(
    project_name: &str,
    input: &FailureTriagePromptInput,
) -> String {
    let hotspot = input.path.as_deref().map_or(String::new(), |p| {
        format!("\n\nInitial hotspot: '{p}'. Start investigation here.")
    });

    format!(
        "## Failure Triage Workflow for '{project_name}'\n\
         Symptom: {symptom}\n\
         \n\
         ### Step 1: Check Recent Changes\n\
         - Call `what_changed(uncommitted=true, code_only=true, include_symbol_diff=true)`\n\
         - If the symptom appeared recently, the bug is likely in uncommitted changes\n\
         - Check `diff_symbols()` — which functions were modified?\n\
         \n\
         ### Step 2: Locate the Failure Point\n\
         - Extract key terms from the symptom (error message, function name, file path)\n\
         - Call `search_text(query=\"<error_text>\")` to find where the error originates\n\
         - Call `inspect_match(path=\"<file>\", line=<N>)` to see the enclosing symbol and context\n\
         \n\
         ### Step 3: Trace the Call Chain\n\
         - Call `get_symbol_context(name=\"<failing_fn>\", sections=[])` for full trace\n\
         - Follow callers: who invokes this function? What inputs does it receive?\n\
         - Follow callees: what dependencies could be causing the failure?\n\
         \n\
         ### Step 4: Check Type Contracts\n\
         - Call `get_symbol_context(name=\"<failing_fn>\", bundle=true)` to see all type deps\n\
         - Did any struct/enum change shape recently? (compare with diff_symbols output)\n\
         - Are there None/null paths not handled?\n\
         \n\
         ### Step 5: Narrow to Root Cause\n\
         - If in changed code: the diff is the likely root cause\n\
         - If in unchanged code: look for dependency changes or environment issues\n\
         - Call `search_text(query=\"<suspect_pattern>\", follow_refs=true)` to check impact\n\
         \n\
         ### Step 6: Report\n\
         Root cause, affected scope (how many callers), suggested fix, and verification steps.{hotspot}",
        symptom = input.symptom
    )
}

fn build_onboard_instructions(project_name: &str, area: Option<&str>) -> String {
    let area_note = area.map_or(String::new(), |a| {
        format!("\n\nStart with the '{a}' area before expanding to the rest.")
    });

    format!(
        "## Codebase Onboarding Workflow for '{project_name}'\n\
         \n\
         ### Step 1: Project Overview (2 minutes)\n\
         - Read the repo map resource (attached) for structure and languages\n\
         - Doctrine: the map orients; the tools prove. Treat the repo map as a ranked starting point, not an inventory.\n\
         - Absence from the map is not absence from the repo - confirm with `search_symbols` / `search_text` before concluding something is missing.\n\
         - Call `get_repo_map(detail=\"tree\", depth=2)` to see directory layout\n\
         - Identify: What language? How many modules? What's the entry point?\n\
         \n\
         ### Step 2: Understand the Architecture (5 minutes)\n\
         - Call `explore(query=\"main entry point\", depth=2)` to find the main function/handler\n\
         - For each top-level directory, call `get_file_context(path=\"<dir>/mod.rs\", sections=[\"outline\"])`\n\
         - Map: what are the 3-5 core modules and what does each do?\n\
         \n\
         ### Step 3: Find the Core Types (3 minutes)\n\
         - Call `search_symbols(kind=\"struct\", limit=15)` to find main data structures\n\
         - Call `search_symbols(kind=\"trait\", limit=10)` to find key abstractions\n\
         - For the top 3 types: `get_symbol(name=\"<type>\")` to read their definition\n\
         \n\
         ### Step 4: Trace a Key Flow (5 minutes)\n\
         - Pick the most important entry point (main, handle_request, process, etc.)\n\
         - Call `get_symbol_context(name=\"<entry>\", sections=[\"dependents\",\"siblings\"])` for full trace\n\
         - Follow 2-3 levels of callees to understand the hot path\n\
         \n\
         ### Step 5: Check Test Patterns (2 minutes)\n\
         - Call `search_files(query=\"test\")` to find test files\n\
         - Call `get_file_context(path=\"<test_file>\", sections=[\"outline\"])` on one test file\n\
         - Note: how are tests organized? What patterns are used?\n\
         \n\
         ### Step 6: Summary\n\
         Produce a concise mental model: purpose, architecture, core types, data flow, test approach.{area_note}"
    )
}

fn build_refactor_instructions(project_name: &str, input: &RefactorPromptInput) -> String {
    let target_note = input
        .target
        .as_deref()
        .map_or(String::new(), |t| format!("\n\nStarting point: '{t}'."));

    format!(
        "## Refactoring Workflow for '{project_name}'\n\
         Goal: {goal}\n\
         \n\
         ### Step 1: Understand Current State\n\
         - Call `search_symbols(query=\"<target>\")` to find the symbol(s) involved\n\
         - Call `get_symbol_context(name=\"<sym>\", bundle=true)` to see the full definition + type deps\n\
         - Call `find_references(name=\"<sym>\", compact=true)` to count all usage sites\n\
         \n\
         ### Step 2: Assess Impact Radius\n\
         - How many files reference this symbol? (from find_references)\n\
         - Call `find_dependents(path=\"<file>\", compact=true)` to see file-level impact\n\
         - Are there >10 callers? If so, this is a high-risk refactor — plan carefully\n\
         \n\
         ### Step 3: Plan the Edit Sequence\n\
         Based on the refactor type:\n\
         - **Rename**: Use `batch_rename(dry_run=true)` to preview all changes\n\
         - **Extract**: Identify the code to extract with `get_symbol`, then `edit_within_symbol` + `insert_symbol`\n\
         - **Restructure**: Use `batch_edit(dry_run=true)` for multi-symbol changes\n\
         - **Delete**: Check `find_references` first — ensure zero callers before `delete_symbol`\n\
         \n\
         ### Step 4: Execute with dry_run First\n\
         - Always run with `dry_run=true` first to preview\n\
         - Review the preview for unintended changes\n\
         - Then execute without dry_run\n\
         \n\
         ### Step 5: Verify\n\
         - Call `analyze_file_impact(path=\"<changed_file>\")` for each modified file\n\
         - Check for stale references in the impact report\n\
         - Search for any remaining old names: `search_text(query=\"<old_name>\")`{target_note}",
        goal = input.goal
    )
}

fn build_debug_instructions(project_name: &str, input: &DebugPromptInput) -> String {
    let path_note = input
        .path
        .as_deref()
        .map_or(String::new(), |p| format!("\n\nSuspected file: '{p}'."));

    format!(
        "## Debugging Workflow for '{project_name}'\n\
         Error: {error}\n\
         \n\
         ### Step 1: Find the Error Origin\n\
         - Extract the key error text or pattern from the error message\n\
         - Call `search_text(query=\"<error_pattern>\")` to find where it's generated\n\
         - If it's a function name: `search_symbols(query=\"<fn_name>\")`\n\
         - Call `inspect_match(path=\"<file>\", line=<N>)` on the match for full context\n\
         \n\
         ### Step 2: Understand the Failing Function\n\
         - Call `get_symbol(path=\"<file>\", name=\"<fn>\")` to read the full body\n\
         - Call `get_symbol_context(name=\"<fn>\", sections=[])` for callers + callees + types\n\
         - Map the data flow: what goes in, what comes out, what can fail?\n\
         \n\
         ### Step 3: Check Recent Changes (is this a regression?)\n\
         - Call `what_changed(uncommitted=true, include_symbol_diff=true)`\n\
         - Did the failing function change recently?\n\
         - Did any of its dependencies change? Check `diff_symbols()`\n\
         \n\
         ### Step 4: Check Callers\n\
         - Call `find_references(name=\"<fn>\", compact=true)` to find all call sites\n\
         - Are callers passing unexpected inputs? Check their code with `get_symbol`\n\
         - Look for pattern mismatches (wrong types, missing error handling)\n\
         \n\
         ### Step 5: Check Dependencies\n\
         - Call `get_symbol_context(name=\"<fn>\", bundle=true)` for all type deps\n\
         - Did any struct/enum change shape? Are there Option/Result paths not handled?\n\
         - Call `search_text(query=\"unwrap()\", glob=\"<suspect_file>\")` to find panic points\n\
         \n\
         ### Step 6: Root Cause Report\n\
         Root cause, evidence trail, affected scope, suggested fix, and how to verify.{path_note}",
        error = input.error
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
            skipped_files: Vec::new(),
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
        let names: Vec<&str> = prompts.iter().map(|prompt| prompt.name.as_str()).collect();
        assert!(names.contains(&"symforge-review"));
        assert!(names.contains(&"symforge-architecture"));
        assert!(names.contains(&"symforge-triage"));
        assert!(names.contains(&"symforge-onboard"));
        assert!(names.contains(&"symforge-refactor"));
        assert!(names.contains(&"symforge-debug"));
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
                rmcp::model::PromptMessageContent::ResourceLink { link }
                    if link.uri == REPO_HEALTH_URI
            )),
            "symforge-review prompt should link repo health"
        );
        assert!(
            result.messages.iter().any(|message| matches!(
                &message.content,
                rmcp::model::PromptMessageContent::ResourceLink { link }
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
}
