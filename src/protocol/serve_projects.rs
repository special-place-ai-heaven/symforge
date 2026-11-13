//! #875: the operator-fixed repositories of one `symforge serve` process.
//!
//! The server `/mcp` dispatches through holds this registry; every tool call and
//! resource read is routed to one entry by its `project` alias before the entry's
//! own dispatch runs, so each answer is judged and labelled by its own repository.
//! A client value is only ever matched against the fixed alias set, never used as
//! a path. The set cannot change at runtime: `index_folder` is refused unless the
//! operator passed `--allow-index-folder`, which startup admits for one repository
//! only.

use std::sync::Arc;

use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Resource,
    ResourceTemplate,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer};
use serde_json::{Map, Value};

use super::SymForgeServer;
use super::result_status::{OutcomeClass, RESULT_STATUS_META_KEY, ResultStatus};
use super::tool_contract;
use crate::live_index::view::{BaseKey, CommitId, IndexBase, WorkingSet};

/// One served repository: its operator alias and its own server (own index,
/// watcher, ledger and knowledge state; nothing is shared between entries).
pub(crate) struct ServedRepo {
    pub(crate) alias: String,
    pub(crate) server: SymForgeServer,
}

pub(crate) struct ServeProjects {
    /// Operator order; the first entry also owns the process-level stores.
    repos: Vec<ServedRepo>,
    allow_index_folder: bool,
    /// The `--project` roots worktree rerouting may write to; `None` when serve
    /// runs on its working directory, where rerouting behaves as before.
    fixed_roots: Option<crate::worktree::ServeRoots>,
}

impl ServeProjects {
    /// `repos` must be non-empty; startup refuses an empty or invalid list.
    pub(crate) fn new(
        repos: Vec<ServedRepo>,
        allow_index_folder: bool,
        fixed_roots: Option<crate::worktree::ServeRoots>,
    ) -> Self {
        debug_assert!(!repos.is_empty(), "serve always has one repository");
        Self {
            repos,
            allow_index_folder,
            fixed_roots,
        }
    }

    pub(crate) fn repos(&self) -> &[ServedRepo] {
        &self.repos
    }

    pub(crate) fn aliases(&self) -> Vec<String> {
        self.repos.iter().map(|repo| repo.alias.clone()).collect()
    }

    fn alias_list(&self) -> String {
        self.aliases().join(", ")
    }

    /// Whether more than one repository is served (the routing-required case).
    pub(crate) fn serves_several(&self) -> bool {
        self.repos.len() > 1
    }

    /// Prompts name one repository and link its resources; on a multi-repository
    /// server they cannot say which, so they are refused rather than emitted.
    pub(crate) fn prompts_refusal(&self) -> String {
        format!(
            "prompts are not served when this server serves several repositories ({}); use \
             the tools with project instead.",
            self.alias_list()
        )
    }

    /// Aliases match case-insensitively, the same folding the startup duplicate
    /// check uses, so no two aliases can match one value.
    fn find(&self, alias: &str) -> Option<&ServedRepo> {
        let folded = alias.to_lowercase();
        self.repos
            .iter()
            .find(|repo| repo.alias.to_lowercase() == folded)
    }

    fn lookup(&self, alias: &str) -> Option<&SymForgeServer> {
        self.find(alias).map(|repo| &repo.server)
    }

    fn missing_project(&self) -> String {
        format!(
            "Error: project_routing: this server serves several repositories; pass project with \
             one of: {}.",
            self.alias_list()
        )
    }

    fn unknown_project(&self, given: &str) -> String {
        format!(
            "Error: project_routing: unknown project '{given}'; this server serves: {}. Pass \
             project with one of those aliases.",
            self.alias_list()
        )
    }

    /// The repository a call names, or the refusal text.
    fn select(&self, project: Option<&str>) -> Result<&SymForgeServer, String> {
        match project {
            Some(alias) => self
                .lookup(alias)
                .ok_or_else(|| self.unknown_project(alias)),
            None if self.repos.len() == 1 => Ok(&self.repos[0].server),
            None => Err(self.missing_project()),
        }
    }

    /// Route one `tools/call` (already past the surface gate) to its repository.
    pub(crate) async fn route_tool_call(
        &self,
        front: &SymForgeServer,
        mut request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let tool = request.name.to_string();
        if tool == "index_folder" && !self.allow_index_folder {
            return Ok(refusal(format!(
                "Error: index_folder refused: the repositories of this server are fixed by the \
                 operator ({}). Pass project to choose one; the operator changes the set by \
                 restarting serve with --project ALIAS=PATH.",
                self.alias_list()
            )));
        }
        let had_arguments = request.arguments.is_some();
        let arguments = request.arguments.get_or_insert_with(Map::new);
        let (project, projects) = match take_selectors(arguments) {
            Ok(selectors) => selectors,
            Err(message) => return Ok(refusal(format!("Error: project_routing: {message}"))),
        };
        if let Some(projects) = projects {
            return Ok(self
                .fan_out(front, &tool, project, projects, arguments.clone())
                .await);
        }
        if project.is_none()
            && self.serves_several()
            && matches!(tool.as_str(), "status" | "health" | "health_compact")
        {
            return Ok(self.server_view_call(front, &tool, arguments));
        }
        let target = match self.select(project.as_deref()) {
            Ok(target) => target,
            Err(message) => return Ok(refusal(message)),
        };
        if !had_arguments && arguments.is_empty() {
            request.arguments = None;
        }
        // Boxed: this future sits inside `call_tool`'s own future.
        let call = Box::pin(target.dispatch_gated_tool_call(request, context));
        match self.fixed_roots.as_ref() {
            Some(roots) => crate::worktree::with_serve_roots(Arc::clone(roots), call).await,
            None => call.await,
        }
    }

    /// `projects` fan-out over the fixed repositories, rebuilt per call from each
    /// one's current published index (so it is never staler than the entries).
    /// Only the code-read verbs: their answers are budgeted inline (no CCR
    /// handle) and typed from the search itself.
    async fn fan_out(
        &self,
        front: &SymForgeServer,
        tool: &str,
        project: Option<String>,
        projects: Vec<String>,
        params: Map<String, Value>,
    ) -> CallToolResponse {
        if !FAN_OUT_TOOLS.contains(&tool) {
            return refusal(format!(
                "Error: project_routing: projects is supported here only on {}; for '{tool}' \
                 pass project with one of: {}.",
                FAN_OUT_TOOLS.join(", "),
                self.alias_list()
            ));
        }
        let mut targeted = Vec::with_capacity(projects.len());
        for alias in &projects {
            if alias == "*" {
                targeted.push(alias.clone());
                continue;
            }
            match self.find(alias) {
                Some(repo) => targeted.push(repo.alias.clone()),
                None => return refusal(self.unknown_project(alias)),
            }
        }
        let targets = match crate::daemon::resolve_targets(project.as_deref(), Some(&targeted), "")
        {
            Ok(targets) => targets,
            Err(message) => return refusal(format!("Error: project_routing: {message}")),
        };
        // The same argument checks a routed call gets.
        let mut arguments = Some(params);
        let (selected_modes, value_notes) =
            match front.refuse_invalid_arguments(tool, &mut arguments) {
                Ok(selected) => selected,
                Err(refused) => return CallToolResponse::Complete(refused),
            };
        let mut working_set = WorkingSet::new();
        for repo in &self.repos {
            let root = repo.server.capture_repo_root().unwrap_or_default();
            let base = IndexBase::new(
                BaseKey::new(root, CommitId::Dirtyless),
                repo.server.index.data_plane().read(),
                1,
            );
            working_set.add(repo.alias.clone(), Arc::new(base));
        }
        let tool = tool.to_string();
        let params = Value::Object(arguments.unwrap_or_default());
        let outcome = tokio::task::spawn_blocking(move || {
            crate::daemon::execute_cross_project_read(&tool, params, targets, &working_set)
        })
        .await;
        match outcome {
            Ok(Ok((text, found))) => {
                // No project evidence: the answer spans repositories and names each
                // in its `── project: <alias> ──` header instead.
                let mut result = CallToolResult::success(vec![ContentBlock::text(text)]);
                let class = if found {
                    OutcomeClass::Found
                } else {
                    OutcomeClass::EmptyResult
                };
                attach_status(&mut result, class);
                tool_contract::append_selection_note(&mut result, &selected_modes);
                tool_contract::append_value_notes(&mut result, &value_notes);
                CallToolResponse::Complete(result)
            }
            Ok(Err(error)) => refusal(format!("Error: {error:#}")),
            Err(join) => refusal(format!("Error: cross-project read failed: {join}")),
        }
    }

    /// `status`, `health` and `health_compact` without `project` on a
    /// multi-repository server. Arguments pass the same checks as any call; one
    /// that acts on or pages a single repository is refused, since this view
    /// has none to apply it to.
    fn server_view_call(
        &self,
        front: &SymForgeServer,
        tool: &str,
        arguments: &mut Map<String, Value>,
    ) -> CallToolResponse {
        let mut checked = Some(std::mem::take(arguments));
        if let Err(refused) = front.refuse_invalid_arguments(tool, &mut checked) {
            return CallToolResponse::Complete(refused);
        }
        let checked = checked.unwrap_or_default();
        if let Some(name) = checked.iter().find_map(|(name, value)| {
            let view_only = tool == "status" && name == "detail";
            (!view_only && !value.is_null()).then_some(name)
        }) {
            return refusal(format!(
                "Error: project_routing: '{name}' applies to one repository; pass project with \
                 one of: {}.",
                self.alias_list()
            ));
        }
        // No project evidence: this answer describes every repository, each row
        // naming its own, so no single repository's evidence would be true.
        let mut result = CallToolResult::success(vec![ContentBlock::text(self.server_view())]);
        attach_status(&mut result, OutcomeClass::Found);
        CallToolResponse::Complete(result)
    }

    fn server_view(&self) -> String {
        let mut lines = vec!["── served repositories ──".to_string()];
        for repo in &self.repos {
            let published = repo.server.index.data_plane().published_state();
            let root = repo
                .server
                .capture_repo_root()
                .map(|root| root.display().to_string())
                .unwrap_or_else(|| "(unbound)".to_string());
            let index = format!("{:?}", published.status).to_lowercase();
            let watcher = format!("{:?}", repo.server.watcher_info.lock().state).to_lowercase();
            lines.push(format!(
                "name={} root={root} index={index} files={} symbols={} watcher={watcher}",
                repo.alias, published.file_count, published.symbol_count
            ));
        }
        lines.push("Pass project=ALIAS for the full report of one repository.".to_string());
        lines.join("\n")
    }

    /// `resources/list` on a multi-repository server: each repository-bound
    /// resource once per alias, carrying `?project=`, so every listed URI reads.
    /// Static resources (tool catalog, glossary) stay listed once.
    pub(crate) fn per_alias_resources(&self, resources: Vec<Resource>) -> Vec<Resource> {
        let mut listed = Vec::new();
        for resource in resources {
            if !resource.uri.starts_with("symforge://repo/") {
                listed.push(resource);
                continue;
            }
            for repo in &self.repos {
                let mut routed = resource.clone();
                if let Ok(mut url) = reqwest::Url::parse(&resource.uri) {
                    url.query_pairs_mut().append_pair("project", &repo.alias);
                    routed.uri = url.to_string();
                }
                routed.name = format!("{} ({})", resource.name, repo.alias);
                listed.push(routed);
            }
        }
        listed
    }

    /// `resources/templates/list` on a multi-repository server: every template
    /// takes the repository as a `project` parameter.
    pub(crate) fn project_templates(
        &self,
        templates: Vec<ResourceTemplate>,
    ) -> Vec<ResourceTemplate> {
        templates
            .into_iter()
            .map(|mut template| {
                let separator = if template.uri_template.contains('?') {
                    '&'
                } else {
                    '?'
                };
                template.uri_template =
                    format!("{}{separator}project={{project}}", template.uri_template);
                template
            })
            .collect()
    }

    /// Route one `resources/read`: a `project` query parameter names the
    /// repository and is removed before the entry parses the URI.
    pub(crate) fn route_resource<'a>(
        &'a self,
        front: &'a SymForgeServer,
        uri: &str,
    ) -> Result<(&'a SymForgeServer, String), ErrorData> {
        let invalid = |message: String| ErrorData::invalid_request(message, None);
        let Ok(mut url) = reqwest::Url::parse(uri) else {
            // Let the entry report the malformed URI exactly as it always has.
            return Ok((front, uri.to_string()));
        };
        let mut project = None;
        let rest: Vec<(String, String)> = url
            .query_pairs()
            .into_owned()
            .filter(|(key, value)| {
                let is_project = key == "project";
                if is_project {
                    project = Some(value.clone());
                }
                !is_project
            })
            .collect();
        let Some(alias) = project else {
            if matches!(url.host_str(), Some("tools" | "glossary")) || self.repos.len() == 1 {
                return Ok((self.select(None).unwrap_or(front), uri.to_string()));
            }
            return Err(invalid(format!(
                "this server serves several repositories; add ?project=ALIAS to the resource \
                 URI with one of: {}.",
                self.alias_list()
            )));
        };
        let target = self
            .lookup(&alias)
            .ok_or_else(|| invalid(self.unknown_project(&alias)))?;
        if rest.is_empty() {
            url.set_query(None);
        } else {
            url.query_pairs_mut().clear().extend_pairs(rest);
        }
        Ok((target, url.to_string()))
    }
}

/// Remove and validate the `project` / `projects` selectors.
fn take_selectors(
    arguments: &mut Map<String, Value>,
) -> Result<(Option<String>, Option<Vec<String>>), String> {
    let project = match arguments.remove("project") {
        None | Some(Value::Null) => None,
        Some(Value::String(alias)) => Some(alias),
        Some(_) => return Err("project must be a string alias.".to_string()),
    };
    let projects = match arguments.remove("projects") {
        None | Some(Value::Null) => None,
        Some(Value::Array(entries)) => Some(
            entries
                .into_iter()
                .map(|entry| match entry {
                    Value::String(alias) => Ok(alias),
                    _ => Err("projects entries must all be string aliases.".to_string()),
                })
                .collect::<Result<Vec<_>, _>>()?,
        ),
        Some(_) => return Err("projects must be an array of aliases.".to_string()),
    };
    Ok((project, projects))
}

/// The code-read verbs `projects` fans out over on `serve`.
const FAN_OUT_TOOLS: [&str; 4] = [
    "search_symbols",
    "search_text",
    "find_references",
    "search_files",
];

fn attach_status(result: &mut CallToolResult, class: OutcomeClass) {
    result.meta.get_or_insert_with(Default::default).0.insert(
        RESULT_STATUS_META_KEY.to_string(),
        serde_json::to_value(ResultStatus::new(class))
            .expect("ResultStatus must serialize to JSON"),
    );
}

/// A typed `isError: true` refusal that names no repository (it judged none).
fn refusal(text: String) -> CallToolResponse {
    let mut result = CallToolResult::error(vec![ContentBlock::text(text)]);
    attach_status(&mut result, OutcomeClass::InvalidRequest);
    CallToolResponse::Complete(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn take_selectors_removes_both_and_rejects_non_strings() {
        let mut arguments = serde_json::json!({"query": "x", "project": "a", "projects": ["b"]})
            .as_object()
            .cloned()
            .expect("object");
        let (project, projects) = take_selectors(&mut arguments).expect("valid selectors");
        assert_eq!(project.as_deref(), Some("a"));
        assert_eq!(projects, Some(vec!["b".to_string()]));
        assert_eq!(arguments.len(), 1, "selectors are removed: {arguments:?}");

        let mut bad = serde_json::json!({"project": 3})
            .as_object()
            .cloned()
            .expect("object");
        assert!(take_selectors(&mut bad).is_err());
        let mut bad = serde_json::json!({"projects": [1]})
            .as_object()
            .cloned()
            .expect("object");
        assert!(take_selectors(&mut bad).is_err());
    }
}
