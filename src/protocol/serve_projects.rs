//! #875: the operator-fixed repositories of one `symforge serve` process.
//!
//! The server `/mcp` dispatches through holds this registry; every tool call and
//! resource read is routed to one entry by its `project` alias before the entry's
//! own dispatch runs, so each answer is judged and labelled by its own repository.
//! A client value is only ever matched against the fixed alias set, never used as
//! a path. The set cannot change at runtime: `index_folder` is refused unless the
//! operator passed `--allow-index-folder`, which startup admits for one repository
//! only.

use std::collections::HashMap;
use std::sync::Arc;

use rmcp::model::{CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer};
use serde_json::{Map, Value};

use super::SymForgeServer;
use super::result_status::{OutcomeClass, RESULT_STATUS_META_KEY, ResultStatus};
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
}

impl ServeProjects {
    /// `repos` must be non-empty; startup refuses an empty or invalid list.
    pub(crate) fn new(repos: Vec<ServedRepo>, allow_index_folder: bool) -> Self {
        debug_assert!(!repos.is_empty(), "serve always has one repository");
        Self {
            repos,
            allow_index_folder,
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

    fn lookup(&self, alias: &str) -> Option<&SymForgeServer> {
        self.repos
            .iter()
            .find(|repo| repo.alias == alias)
            .map(|repo| &repo.server)
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
            && self.repos.len() > 1
            && tool == "status"
            && arguments.get("detail").and_then(Value::as_str) == Some("projects")
        {
            return Ok(CallToolResponse::Complete(CallToolResult::success(vec![
                ContentBlock::text(self.inventory()),
            ])));
        }
        let target = match self.select(project.as_deref()) {
            Ok(target) => target,
            Err(message) => return Ok(refusal(message)),
        };
        if !had_arguments && arguments.is_empty() {
            request.arguments = None;
        }
        // Boxed: this future sits inside `call_tool`'s own future.
        Box::pin(target.dispatch_gated_tool_call(request, context)).await
    }

    /// `projects` fan-out over the fixed repositories, rebuilt per call from each
    /// one's current published index (so it is never staler than the entries).
    async fn fan_out(
        &self,
        front: &SymForgeServer,
        tool: &str,
        project: Option<String>,
        projects: Vec<String>,
        params: Map<String, Value>,
    ) -> CallToolResponse {
        if !crate::daemon::is_cross_project_read_verb(tool) {
            return refusal(format!(
                "Error: project_routing: projects is supported here only on search_symbols, \
                 search_text, find_references, search_files, search_knowledge and \
                 review_knowledge; for '{tool}' pass project with one of: {}.",
                self.alias_list()
            ));
        }
        if let Some(unknown) = projects
            .iter()
            .find(|alias| alias.as_str() != "*" && self.lookup(alias).is_none())
        {
            return refusal(self.unknown_project(unknown));
        }
        let targets = match crate::daemon::resolve_targets(project.as_deref(), Some(&projects), "")
        {
            Ok(targets) => targets,
            Err(message) => return refusal(format!("Error: project_routing: {message}")),
        };
        let mut working_set = WorkingSet::new();
        let mut indexes = HashMap::new();
        for repo in &self.repos {
            let root = repo.server.capture_repo_root().unwrap_or_default();
            let base = IndexBase::new(
                BaseKey::new(root, CommitId::Dirtyless),
                repo.server.index.data_plane().read(),
                1,
            );
            working_set.add(repo.alias.clone(), Arc::new(base));
            indexes.insert(repo.alias.clone(), repo.server.index.clone());
        }
        let tool = tool.to_string();
        let ccr_server = front.clone();
        let outcome = tokio::task::spawn_blocking(move || {
            crate::daemon::serve_cross_project_read(
                &tool,
                Value::Object(params),
                targets,
                &working_set,
                &indexes,
                &ccr_server,
            )
        })
        .await;
        match outcome {
            Ok(Ok(text)) => {
                CallToolResponse::Complete(CallToolResult::success(vec![ContentBlock::text(text)]))
            }
            Ok(Err(error)) => refusal(format!("Error: {error:#}")),
            Err(join) => refusal(format!("Error: cross-project read failed: {join}")),
        }
    }

    /// `status {detail: projects}` without `project`: one row per repository.
    fn inventory(&self) -> String {
        let mut lines = vec!["── projects ──".to_string()];
        for repo in &self.repos {
            let published = repo.server.index.data_plane().published_state();
            let root = repo
                .server
                .capture_repo_root()
                .map(|root| root.display().to_string())
                .unwrap_or_else(|| "(unbound)".to_string());
            let watcher = format!("{:?}", repo.server.watcher_info.lock().state).to_lowercase();
            lines.push(format!(
                "name={} root={root} files={} symbols={} watcher={watcher}",
                repo.alias, published.file_count, published.symbol_count
            ));
        }
        lines.join("\n")
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

/// A typed `isError: true` refusal that names no repository (it judged none).
fn refusal(text: String) -> CallToolResponse {
    let mut result = CallToolResult::error(vec![ContentBlock::text(text)]);
    result.meta.get_or_insert_with(Default::default).0.insert(
        RESULT_STATUS_META_KEY.to_string(),
        serde_json::to_value(ResultStatus::new(OutcomeClass::InvalidRequest))
            .expect("ResultStatus must serialize to JSON"),
    );
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
