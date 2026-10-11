use reqwest::Url;
use rmcp::ErrorData as McpError;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{ReadResourceResult, Resource, ResourceContents, ResourceTemplate};

use super::SymForgeServer;
#[cfg(test)]
use super::tools::classify_compact_tool_output;
#[cfg(test)]
use crate::protocol::result_status::OutcomeClass;
use crate::protocol::result_status::{ReadOutcome, with_read_outcome_scope};
use crate::protocol::tools::{
    GetFileContentInput, GetFileContextInput, GetRepoMapInput, GetSymbolContextInput,
    GetSymbolInput, WhatChangedInput,
};

pub(crate) const REPO_HEALTH_URI: &str = "symforge://repo/health";
pub(crate) const REPO_OUTLINE_URI: &str = "symforge://repo/outline";
pub(crate) const REPO_MAP_URI: &str = "symforge://repo/map";
pub(crate) const REPO_CHANGES_URI: &str = "symforge://repo/changes/uncommitted";
pub(crate) const TOOLS_CATALOG_URI: &str = "symforge://tools/catalog";
pub(crate) const GLOSSARY_URI: &str = "symforge://glossary";

pub(crate) const FILE_CONTEXT_TEMPLATE: &str =
    "symforge://file/context?path={path}&max_tokens={max_tokens}";
pub(crate) const FILE_CONTENT_TEMPLATE: &str = "symforge://file/content?path={path}&start_line={start_line}&end_line={end_line}&around_line={around_line}&around_match={around_match}&match_occurrence={match_occurrence}&context_lines={context_lines}&show_line_numbers={show_line_numbers}&header={header}";
pub(crate) const SYMBOL_DETAIL_TEMPLATE: &str =
    "symforge://symbol/detail?path={path}&name={name}&kind={kind}";
pub(crate) const SYMBOL_CONTEXT_TEMPLATE: &str =
    "symforge://symbol/context?name={name}&file={file}";

enum ResourceRequest {
    RepoHealth,
    RepoOutline,
    RepoMap,
    RepoChangesUncommitted,
    ToolsCatalog,
    Glossary,
    FileContext {
        path: String,
        max_tokens: Option<u64>,
    },
    FileContent {
        path: String,
        start_line: Option<u32>,
        end_line: Option<u32>,
        around_line: Option<u32>,
        around_match: Option<String>,
        match_occurrence: Option<u32>,
        context_lines: Option<u32>,
        show_line_numbers: Option<bool>,
        header: Option<bool>,
    },
    SymbolDetail {
        path: String,
        name: String,
        kind: Option<String>,
    },
    SymbolContext {
        name: String,
        file: Option<String>,
    },
}

impl ResourceRequest {
    /// The tool whose rendered output answers this request, for the outcome
    /// classifier. `None` for resources that report state rather than read it.
    fn outcome_tool(&self) -> Option<&'static str> {
        match self {
            Self::RepoOutline | Self::RepoMap => Some("get_repo_map"),
            Self::RepoChangesUncommitted => Some("what_changed"),
            Self::FileContext { .. } => Some("get_file_context"),
            Self::FileContent { .. } => Some("get_file_content"),
            Self::SymbolDetail { .. } => Some("get_symbol"),
            Self::SymbolContext { .. } => Some("get_symbol_context"),
            Self::RepoHealth | Self::ToolsCatalog | Self::Glossary => None,
        }
    }
}

impl SymForgeServer {
    pub(crate) fn resource_definitions(&self) -> Vec<Resource> {
        vec![
            make_resource(
                REPO_HEALTH_URI,
                "repo-health",
                "Repository health",
                "Live health report for the current project runtime.",
            ),
            make_resource(
                REPO_OUTLINE_URI,
                "repo-outline",
                "Repository outline",
                "Compact file-level outline for the current project.",
            ),
            make_resource(
                REPO_MAP_URI,
                "repo-map",
                "Repository map",
                "Compact directory and symbol map for the current project.",
            ),
            make_resource(
                REPO_CHANGES_URI,
                "repo-changes-uncommitted",
                "Uncommitted changes",
                "Changed files in the current worktree.",
            ),
            make_resource(
                TOOLS_CATALOG_URI,
                "tools-catalog",
                "Tool catalog",
                "Workflow-grouped index of SymForge's tools.",
            ),
            make_resource(
                GLOSSARY_URI,
                "glossary",
                "SymForge glossary",
                "Definitions of SymForge concepts: project-target vs within-project filter, intents, server/background-service/install, base+overlay, that economy figures are estimates, and status fields.",
            ),
        ]
    }

    pub(crate) fn resource_template_definitions(&self) -> Vec<ResourceTemplate> {
        vec![
            make_resource_template(
                FILE_CONTEXT_TEMPLATE,
                "file-context",
                "File context",
                "File outline plus key external references.",
            ),
            make_resource_template(
                FILE_CONTENT_TEMPLATE,
                "file-content",
                "File content",
                "Cached file content with optional line range or contextual excerpt.",
            ),
            make_resource_template(
                SYMBOL_DETAIL_TEMPLATE,
                "symbol-detail",
                "Symbol detail",
                "Definition body for a symbol in a file.",
            ),
            make_resource_template(
                SYMBOL_CONTEXT_TEMPLATE,
                "symbol-context",
                "Symbol context",
                "Grouped references for a symbol with enclosing annotations.",
            ),
        ]
    }

    pub(crate) async fn read_resource_uri(
        &self,
        uri: &str,
    ) -> Result<ReadResourceResult, McpError> {
        let request =
            parse_resource_uri(uri).map_err(|error| McpError::invalid_params(error, None))?;
        let tool = request.outcome_tool();
        let (text, recorded) = with_read_outcome_scope(self.render_resource_text(request)).await;
        let text = text.map_err(|error| McpError::invalid_params(error, None))?;
        if tool.is_some() {
            // A resource backed by a covered read tool fails closed if that
            // tool did not record a typed outcome.
            let outcome = recorded.unwrap_or(ReadOutcome::Unavailable { retryable: false });
            if let Some(error) = resource_read_error(outcome, uri, text.clone()) {
                return Err(error);
            }
        }

        // FR-312 / INV-4 (spec 025): live per-workspace state behind bearer
        // auth — never eligible for a shared cache, and any nonzero TTL would
        // keep serving content the spec-023 admission gate has since refused.
        Ok(ReadResourceResult::new(vec![
            ResourceContents::text(text, uri.to_string()).with_mime_type("text/markdown"),
        ])
        .with_ttl_ms(0)
        .with_cache_scope(rmcp::model::CacheScope::Private))
    }

    async fn render_resource_text(&self, request: ResourceRequest) -> Result<String, String> {
        let text = match request {
            ResourceRequest::RepoHealth => {
                self.health(Parameters(crate::protocol::tools::HealthInput::default()))
                    .await
            }
            ResourceRequest::RepoOutline => {
                self.get_repo_map(Parameters(GetRepoMapInput {
                    project: None,
                    detail: Some("full".to_string()),
                    path: None,
                    depth: None,
                    max_files: None,
                    estimate: None,
                    max_tokens: None,
                }))
                .await
            }
            ResourceRequest::RepoMap => {
                self.get_repo_map(Parameters(GetRepoMapInput {
                    project: None,
                    detail: None,
                    path: None,
                    depth: None,
                    max_files: None,
                    estimate: None,
                    max_tokens: None,
                }))
                .await
            }
            ResourceRequest::RepoChangesUncommitted => {
                self.what_changed(Parameters(WhatChangedInput {
                    project: None,
                    since: None,
                    git_ref: None,
                    uncommitted: None,
                    path_prefix: None,
                    language: None,
                    code_only: None,
                    include_symbol_diff: None,
                    estimate: None,
                    max_tokens: None,
                }))
                .await
            }
            ResourceRequest::ToolsCatalog => crate::protocol::smart_query::render_tool_catalog(),
            ResourceRequest::Glossary => crate::protocol::smart_query::render_glossary(),
            ResourceRequest::FileContext { path, max_tokens } => {
                self.get_file_context(Parameters(GetFileContextInput {
                    project: None,
                    path,
                    max_tokens,
                    sections: None,
                    estimate: None,
                    force_refresh: None,
                }))
                .await
            }
            ResourceRequest::FileContent {
                path,
                start_line,
                end_line,
                around_line,
                around_match,
                match_occurrence,
                context_lines,
                show_line_numbers,
                header,
            } => {
                self.get_file_content(Parameters(GetFileContentInput {
                    project: None,
                    path,
                    mode: None,
                    start_line,
                    end_line,
                    chunk_index: None,
                    max_lines: None,
                    around_line,
                    around_match,
                    match_occurrence,
                    around_symbol: None,
                    symbol_line: None,
                    context_lines,
                    show_line_numbers,
                    header,
                    estimate: None,
                    offset: None,
                    limit: None,
                    max_tokens: None,
                    force_refresh: None,
                }))
                .await
            }
            ResourceRequest::SymbolDetail { path, name, kind } => {
                self.get_symbol(Parameters(GetSymbolInput {
                    project: None,
                    path,
                    name,
                    kind,
                    symbol_line: None,
                    targets: None,
                    estimate: None,
                    max_tokens: None,
                    force_refresh: None,
                }))
                .await
            }
            ResourceRequest::SymbolContext { name, file } => {
                self.get_symbol_context(Parameters(GetSymbolContextInput {
                    project: None,
                    name,
                    file,
                    path: None,
                    symbol_kind: None,
                    symbol_line: None,
                    verbosity: None,
                    bundle: None,
                    sections: None,
                    max_tokens: None,
                    estimate: None,
                }))
                .await
            }
        };

        Ok(text)
    }
}

/// The MCP error for a read that did not succeed; `None` for a successful read.
/// An unready index keeps the shape the stdio front gives a resource read while
/// the runtime is starting (`internal_error` with `retryable`).
fn resource_read_error(outcome: ReadOutcome, uri: &str, text: String) -> Option<McpError> {
    Some(match outcome {
        ReadOutcome::Found | ReadOutcome::Empty => return None,
        ReadOutcome::NotFound => {
            McpError::resource_not_found(text, Some(serde_json::json!({ "uri": uri })))
        }
        ReadOutcome::InvalidRequest => {
            McpError::invalid_params(text, Some(serde_json::json!({ "uri": uri })))
        }
        ReadOutcome::Ambiguous => McpError::invalid_params(
            text,
            Some(serde_json::json!({ "uri": uri, "reason": "ambiguous" })),
        ),
        ReadOutcome::Unavailable { retryable } => McpError::internal_error(
            text,
            Some(serde_json::json!({
                "uri": uri,
                "reason": "index_unavailable",
                "retryable": retryable,
            })),
        ),
    })
}

pub(crate) fn repo_health_resource() -> Resource {
    make_resource(
        REPO_HEALTH_URI,
        "repo-health",
        "Repository health",
        "Live health report for the current project runtime.",
    )
}

pub(crate) fn repo_outline_resource() -> Resource {
    make_resource(
        REPO_OUTLINE_URI,
        "repo-outline",
        "Repository outline",
        "Compact file-level outline for the current project.",
    )
}

pub(crate) fn repo_map_resource() -> Resource {
    make_resource(
        REPO_MAP_URI,
        "repo-map",
        "Repository map",
        "Compact directory and symbol map for the current project.",
    )
}

pub(crate) fn repo_changes_resource() -> Resource {
    make_resource(
        REPO_CHANGES_URI,
        "repo-changes-uncommitted",
        "Uncommitted changes",
        "Changed files in the current worktree.",
    )
}

pub(crate) fn tools_catalog_resource() -> Resource {
    make_resource(
        TOOLS_CATALOG_URI,
        "tools-catalog",
        "Tool catalog",
        "Workflow-grouped index of SymForge's tools.",
    )
}

pub(crate) fn file_context_resource(path: &str, max_tokens: Option<u64>) -> Resource {
    let uri = build_uri(
        "symforge://file/context",
        &[
            ("path", Some(path.to_string())),
            ("max_tokens", max_tokens.map(|v| v.to_string())),
        ],
    );
    make_resource(
        &uri,
        "file-context",
        "File context",
        "File outline plus key external references.",
    )
}

fn make_resource(uri: &str, name: &str, title: &str, description: &str) -> Resource {
    // rmcp 2.0: `Resource` is a flat struct (no more `Annotated<RawResource>` /
    // `AnnotateAble::no_annotation()`); build it directly.
    Resource::new(uri.to_string(), name.to_string())
        .with_title(title.to_string())
        .with_description(description.to_string())
        .with_mime_type("text/markdown")
}

fn make_resource_template(
    uri_template: &str,
    name: &str,
    title: &str,
    description: &str,
) -> ResourceTemplate {
    // rmcp 2.0: `ResourceTemplate` is a flat struct; build it directly.
    ResourceTemplate::new(uri_template.to_string(), name.to_string())
        .with_title(title.to_string())
        .with_description(description.to_string())
        .with_mime_type("text/markdown")
}

fn build_uri(base: &str, params: &[(&str, Option<String>)]) -> String {
    let mut url = Url::parse(base).expect("static symforge resource URI must parse");
    {
        let mut query = url.query_pairs_mut();
        for (key, value) in params {
            if let Some(value) = value {
                query.append_pair(key, value);
            }
        }
    }
    url.to_string()
}

fn parse_resource_uri(uri: &str) -> Result<ResourceRequest, String> {
    let url = Url::parse(uri).map_err(|error| format!("invalid resource URI: {error}"))?;
    if url.scheme() != "symforge" {
        return Err(format!("unsupported resource scheme '{}'", url.scheme()));
    }

    let query: std::collections::HashMap<String, String> = url.query_pairs().into_owned().collect();

    match (url.host_str(), url.path()) {
        (Some("repo"), "/health") => Ok(ResourceRequest::RepoHealth),
        (Some("repo"), "/outline") => Ok(ResourceRequest::RepoOutline),
        (Some("repo"), "/map") => Ok(ResourceRequest::RepoMap),
        (Some("repo"), "/changes/uncommitted") => Ok(ResourceRequest::RepoChangesUncommitted),
        (Some("tools"), "/catalog") => Ok(ResourceRequest::ToolsCatalog),
        // `symforge://glossary` has an empty path (the concept is the host).
        (Some("glossary"), "" | "/") => Ok(ResourceRequest::Glossary),
        (Some("file"), "/context") => Ok(ResourceRequest::FileContext {
            path: required_query(&query, "path")?,
            max_tokens: optional_query(&query, "max_tokens").transpose()?,
        }),
        (Some("file"), "/content") => Ok(ResourceRequest::FileContent {
            path: required_query(&query, "path")?,
            start_line: optional_query(&query, "start_line").transpose()?,
            end_line: optional_query(&query, "end_line").transpose()?,
            around_line: optional_query(&query, "around_line").transpose()?,
            around_match: optional_text(&query, "around_match"),
            match_occurrence: optional_query(&query, "match_occurrence").transpose()?,
            context_lines: optional_query(&query, "context_lines").transpose()?,
            show_line_numbers: optional_query(&query, "show_line_numbers").transpose()?,
            header: optional_query(&query, "header").transpose()?,
        }),
        (Some("symbol"), "/detail") => Ok(ResourceRequest::SymbolDetail {
            path: required_query(&query, "path")?,
            name: required_query(&query, "name")?,
            kind: optional_text(&query, "kind"),
        }),
        (Some("symbol"), "/context") => Ok(ResourceRequest::SymbolContext {
            name: required_query(&query, "name")?,
            file: optional_text(&query, "file"),
        }),
        (host, path) => Err(format!(
            "unsupported SymForge resource target '{}{}'",
            host.unwrap_or("<none>"),
            path
        )),
    }
}

fn required_query(
    query: &std::collections::HashMap<String, String>,
    key: &str,
) -> Result<String, String> {
    query
        .get(key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("resource URI missing required query parameter '{key}'"))
}

fn optional_text(query: &std::collections::HashMap<String, String>, key: &str) -> Option<String> {
    query
        .get(key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn optional_query<T>(
    query: &std::collections::HashMap<String, String>,
    key: &str,
) -> Option<Result<T, String>>
where
    T: std::str::FromStr,
    <T as std::str::FromStr>::Err: std::fmt::Display,
{
    optional_text(query, key).map(|raw| {
        raw.parse::<T>()
            .map_err(|error| format!("invalid value for '{key}': {error}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use crate::domain::{LanguageId, SymbolKind, SymbolRecord};
    use crate::live_index::store::{CircuitBreakerState, IndexedFile, LiveIndex, ParseStatus};
    use crate::protocol::SymForgeServer;
    use crate::watcher::WatcherInfo;

    fn make_server_with_file(path: &str, content: &[u8]) -> SymForgeServer {
        make_server_with_file_in_state(
            path,
            content,
            crate::live_index::store::SnapshotVerifyState::NotNeeded,
        )
    }

    fn make_server_with_file_in_state(
        path: &str,
        content: &[u8],
        snapshot_verify_state: crate::live_index::store::SnapshotVerifyState,
    ) -> SymForgeServer {
        make_server_with_files(&[(path, content, "main")], snapshot_verify_state)
    }

    /// One indexed file per `(path, content, symbol)`, each defining one
    /// function symbol over its first ten bytes.
    fn make_server_with_files(
        entries: &[(&str, &[u8], &str)],
        snapshot_verify_state: crate::live_index::store::SnapshotVerifyState,
    ) -> SymForgeServer {
        let mut files = HashMap::new();
        for (path, content, name) in entries {
            let byte_range = (0, 10);
            let symbol = SymbolRecord {
                name: name.to_string(),
                kind: SymbolKind::Function,
                depth: 0,
                sort_order: 0,
                byte_range,
                item_byte_range: Some(byte_range),
                line_range: (
                    1,
                    content.iter().filter(|&&byte| byte == b'\n').count() as u32 + 1,
                ),
                doc_byte_range: None,
            };
            let file = IndexedFile {
                relative_path: path.to_string(),
                language: LanguageId::Rust,
                classification: crate::domain::FileClassification::for_code_path(path),
                content: content.to_vec(),
                symbols: vec![symbol],
                parse_status: ParseStatus::Parsed,
                parse_diagnostic: None,
                byte_len: content.len() as u64,
                content_hash: "test".to_string(),
                references: vec![],
                alias_map: HashMap::new(),
                mtime_secs: 0,
            };
            files.insert(path.to_string(), std::sync::Arc::new(file));
        }
        let mut index = LiveIndex {
            files,
            loaded_at: Instant::now(),
            loaded_at_system: std::time::SystemTime::now(),
            load_duration: Duration::from_millis(10),
            cb_state: CircuitBreakerState::new(0.20),
            is_empty: false,
            load_source: crate::live_index::store::IndexLoadSource::FreshLoad,
            snapshot_verify_state,
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
        index.rebuild_reverse_index();
        index.rebuild_path_indices();
        SymForgeServer::new(
            crate::live_index::SharedIndexHandle::shared(index),
            "test_project".to_string(),
            Arc::new(Mutex::new(WatcherInfo::default())),
            None,
            None,
        )
    }

    fn make_server() -> SymForgeServer {
        make_server_with_file("src/main.rs", b"fn main() {}")
    }

    #[test]
    fn test_resource_definitions_include_repo_surfaces() {
        let server = make_server();
        let resources = server.resource_definitions();
        let uris: Vec<&str> = resources
            .iter()
            .map(|resource| resource.uri.as_str())
            .collect();
        assert!(uris.contains(&REPO_HEALTH_URI));
        assert!(uris.contains(&REPO_MAP_URI));
        assert!(uris.contains(&TOOLS_CATALOG_URI));
    }

    #[tokio::test]
    async fn test_read_tools_catalog_resource() {
        let server = make_server();
        let result = server
            .read_resource_uri(TOOLS_CATALOG_URI)
            .await
            .expect("read resource");
        let text = match &result.contents[0] {
            ResourceContents::TextResourceContents { text, .. } => text,
            other => panic!("expected text resource, got {other:?}"),
        };
        assert!(text.contains("impact-analysis"), "catalog text: {text}");
        assert!(text.contains("analyze_file_impact"), "catalog text: {text}");
    }

    #[test]
    fn test_resource_templates_include_file_and_symbol_templates() {
        let server = make_server();
        let templates = server.resource_template_definitions();
        let uris: Vec<&str> = templates
            .iter()
            .map(|template| template.uri_template.as_str())
            .collect();
        assert!(uris.contains(&FILE_CONTENT_TEMPLATE));
        assert!(uris.contains(&FILE_CONTEXT_TEMPLATE));
        assert!(uris.contains(&SYMBOL_CONTEXT_TEMPLATE));
    }

    #[tokio::test]
    async fn test_read_static_repo_map_resource() {
        let server = make_server();
        let result = server
            .read_resource_uri(REPO_MAP_URI)
            .await
            .expect("read resource");
        let text = match &result.contents[0] {
            ResourceContents::TextResourceContents { text, .. } => text,
            other => panic!("expected text resource, got {other:?}"),
        };
        assert!(text.contains("Index: 1 files, 1 symbols"));
        // Orientation doctrine must ride the compact map footer into the
        // symforge://repo/map resource body (delegates to get_repo_map compact).
        assert!(
            text.contains("map orients"),
            "repo map resource must embed the 'map orients' doctrine: {text}"
        );
        assert!(
            text.contains("not absence from the repo"),
            "repo map resource must embed the 'not absence' doctrine: {text}"
        );
    }

    #[tokio::test]
    async fn test_read_templated_file_context_resource() {
        let server = make_server();
        let uri = build_uri(
            "symforge://file/context",
            &[("path", Some("src/main.rs".to_string()))],
        );
        let result = server.read_resource_uri(&uri).await.expect("read resource");
        let text = match &result.contents[0] {
            ResourceContents::TextResourceContents { text, .. } => text,
            other => panic!("expected text resource, got {other:?}"),
        };
        assert!(text.contains("src/main.rs"));
    }

    #[tokio::test]
    async fn test_read_templated_file_content_resource_with_ordinary_read_flags() {
        let server = make_server();
        let uri = build_uri(
            "symforge://file/content",
            &[
                ("path", Some("src/main.rs".to_string())),
                ("show_line_numbers", Some("true".to_string())),
                ("header", Some("true".to_string())),
            ],
        );
        let result = server.read_resource_uri(&uri).await.expect("read resource");
        let text = match &result.contents[0] {
            ResourceContents::TextResourceContents { text, .. } => text,
            other => panic!("expected text resource, got {other:?}"),
        };
        assert_eq!(text, "src/main.rs\n1: fn main() {}");
    }

    #[tokio::test]
    async fn test_read_templated_file_content_resource_with_around_line_context() {
        let server =
            make_server_with_file("src/main.rs", b"line 1\nline 2\nline 3\nline 4\nline 5");
        let uri = build_uri(
            "symforge://file/content",
            &[
                ("path", Some("src/main.rs".to_string())),
                ("around_line", Some("3".to_string())),
                ("context_lines", Some("1".to_string())),
            ],
        );
        let result = server.read_resource_uri(&uri).await.expect("read resource");
        let text = match &result.contents[0] {
            ResourceContents::TextResourceContents { text, .. } => text,
            other => panic!("expected text resource, got {other:?}"),
        };
        assert_eq!(text, "2: line 2\n3: line 3\n4: line 4");
    }

    #[tokio::test]
    async fn test_read_templated_file_content_resource_with_around_match_context() {
        let server = make_server_with_file(
            "src/main.rs",
            b"line 1\nTODO first\nline 3\nTODO second\nline 5",
        );
        let uri = build_uri(
            "symforge://file/content",
            &[
                ("path", Some("src/main.rs".to_string())),
                ("around_match", Some("todo".to_string())),
                ("context_lines", Some("1".to_string())),
            ],
        );
        let result = server.read_resource_uri(&uri).await.expect("read resource");
        let text = match &result.contents[0] {
            ResourceContents::TextResourceContents { text, .. } => text,
            other => panic!("expected text resource, got {other:?}"),
        };
        assert_eq!(text, "1: line 1\n2: TODO first\n3: line 3");
    }

    #[tokio::test]
    async fn test_read_templated_resource_for_missing_target_is_resource_not_found() {
        let server = make_server();
        let uris = [
            build_uri(
                "symforge://file/content",
                &[("path", Some("src/missing.rs".to_string()))],
            ),
            build_uri(
                "symforge://file/context",
                &[("path", Some("src/missing.rs".to_string()))],
            ),
            build_uri(
                "symforge://symbol/detail",
                &[
                    ("path", Some("src/missing.rs".to_string())),
                    ("name", Some("main".to_string())),
                ],
            ),
            build_uri(
                "symforge://symbol/detail",
                &[
                    ("path", Some("src/main.rs".to_string())),
                    ("name", Some("no_such_symbol".to_string())),
                ],
            ),
            build_uri(
                "symforge://symbol/context",
                &[
                    ("name", Some("no_such_symbol".to_string())),
                    ("file", Some("src/main.rs".to_string())),
                ],
            ),
        ];
        for uri in uris {
            let error = server
                .read_resource_uri(&uri)
                .await
                .expect_err("missing target must not read as success");
            assert_eq!(
                error.code,
                rmcp::model::ErrorCode::RESOURCE_NOT_FOUND,
                "{uri}"
            );
            assert_eq!(error.data, Some(serde_json::json!({ "uri": uri })));
        }
    }

    #[tokio::test]
    async fn test_read_existing_file_whose_content_looks_like_a_miss_is_success() {
        let server = make_server_with_file("src/main.rs", b"File not found: decoy");
        let uri = build_uri(
            "symforge://file/content",
            &[
                ("path", Some("src/main.rs".to_string())),
                ("show_line_numbers", Some("false".to_string())),
                ("header", Some("false".to_string())),
            ],
        );
        let result = server
            .read_resource_uri(&uri)
            .await
            .expect("an existing file is never resource-not-found");
        let text = match &result.contents[0] {
            ResourceContents::TextResourceContents { text, .. } => text,
            other => panic!("expected text resource, got {other:?}"),
        };
        assert_eq!(text, "File not found: decoy");
    }

    #[tokio::test]
    async fn test_read_existing_symbol_whose_body_looks_like_a_miss_is_success() {
        // The fixture symbol `main` spans bytes 0..10, so its body is `No symbol `.
        let server = make_server_with_file("src/main.rs", b"No symbol here");
        let uri = build_uri(
            "symforge://symbol/detail",
            &[
                ("path", Some("src/main.rs".to_string())),
                ("name", Some("main".to_string())),
            ],
        );
        let result = server
            .read_resource_uri(&uri)
            .await
            .expect("an existing symbol is never resource-not-found");
        let text = match &result.contents[0] {
            ResourceContents::TextResourceContents { text, .. } => text,
            other => panic!("expected text resource, got {other:?}"),
        };
        assert!(text.starts_with("No symbol "), "{text}");
    }

    #[tokio::test]
    async fn test_read_existing_unreferenced_symbol_context_is_success() {
        let server = make_server();
        let uri = build_uri(
            "symforge://symbol/context",
            &[
                ("name", Some("main".to_string())),
                ("file", Some("src/main.rs".to_string())),
            ],
        );
        let result = server
            .read_resource_uri(&uri)
            .await
            .expect("an existing symbol with no references is a successful read");
        let text = match &result.contents[0] {
            ResourceContents::TextResourceContents { text, .. } => text,
            other => panic!("expected text resource, got {other:?}"),
        };
        assert!(text.contains("No references found"), "{text}");
    }

    #[tokio::test]
    async fn test_read_invalid_request_resource_is_invalid_params() {
        let server = make_server();
        let uri = build_uri(
            "symforge://file/content",
            &[
                ("path", Some("src/main.rs".to_string())),
                ("start_line", Some("5".to_string())),
                ("end_line", Some("2".to_string())),
            ],
        );
        let error = server
            .read_resource_uri(&uri)
            .await
            .expect_err("an invalid request must not read as success");
        assert_eq!(error.code, rmcp::model::ErrorCode::INVALID_PARAMS);
        assert_eq!(error.data, Some(serde_json::json!({ "uri": uri })));
    }

    #[tokio::test]
    async fn test_read_resource_while_index_loads_is_retryable_internal_error() {
        let server = make_server_with_file_in_state(
            "src/main.rs",
            b"fn main() {}",
            crate::live_index::store::SnapshotVerifyState::Pending,
        );
        let uri = build_uri(
            "symforge://file/content",
            &[("path", Some("src/main.rs".to_string()))],
        );
        let error = server
            .read_resource_uri(&uri)
            .await
            .expect_err("a loading index must not read as success");
        assert_eq!(error.code, rmcp::model::ErrorCode::INTERNAL_ERROR);
        assert_eq!(
            error.data,
            Some(serde_json::json!({
                "uri": uri,
                "reason": "index_unavailable",
                "retryable": true,
            }))
        );
    }

    /// `file` filters which references are shown; it does not name the
    /// symbol's location. A symbol defined in another file and unreferenced
    /// in the filtered one is "No references found", through the tool and the
    /// resource alike. A name defined nowhere is the miss.
    #[tokio::test]
    async fn test_symbol_context_file_filter_misses_only_a_symbol_defined_nowhere() {
        let server = make_server_with_files(
            &[
                ("src/a.rs", b"fn helper() {}", "helper"),
                ("src/b.rs", b"fn other() {}", "other"),
            ],
            crate::live_index::store::SnapshotVerifyState::NotNeeded,
        );
        let tool = server
            .get_symbol_context(Parameters(GetSymbolContextInput {
                project: None,
                name: "helper".to_string(),
                file: Some("src/b.rs".to_string()),
                path: None,
                symbol_kind: None,
                symbol_line: None,
                verbosity: None,
                bundle: None,
                sections: None,
                max_tokens: None,
                estimate: None,
            }))
            .await;
        assert!(tool.contains("No references found"), "{tool}");
        let defined_elsewhere = build_uri(
            "symforge://symbol/context",
            &[
                ("name", Some("helper".to_string())),
                ("file", Some("src/b.rs".to_string())),
            ],
        );
        let result = server
            .read_resource_uri(&defined_elsewhere)
            .await
            .expect("a symbol defined elsewhere is not a miss");
        let text = match &result.contents[0] {
            ResourceContents::TextResourceContents { text, .. } => text,
            other => panic!("expected text resource, got {other:?}"),
        };
        assert!(text.contains("No references found"), "{text}");
        let nowhere = build_uri(
            "symforge://symbol/context",
            &[
                ("name", Some("defined_nowhere".to_string())),
                ("file", Some("src/b.rs".to_string())),
            ],
        );
        let error = server
            .read_resource_uri(&nowhere)
            .await
            .expect_err("a name defined nowhere is a miss");
        assert_eq!(error.code, rmcp::model::ErrorCode::RESOURCE_NOT_FOUND);
    }

    /// A name-only symbol context: a name defined nowhere is a miss through the
    /// tool outcome and the resource; an existing unreferenced symbol is not.
    #[tokio::test]
    async fn test_name_only_symbol_context_misses_only_a_symbol_defined_nowhere() {
        let server = make_server();
        let input = |name: &str| GetSymbolContextInput {
            project: None,
            name: name.to_string(),
            file: None,
            path: None,
            symbol_kind: None,
            symbol_line: None,
            verbosity: None,
            bundle: None,
            sections: None,
            max_tokens: None,
            estimate: None,
        };
        let tool = server
            .get_symbol_context(Parameters(input("defined_nowhere")))
            .await;
        assert_eq!(
            classify_compact_tool_output("get_symbol_context", &tool),
            OutcomeClass::NotFound,
            "{tool}"
        );
        let nowhere = build_uri(
            "symforge://symbol/context",
            &[("name", Some("defined_nowhere".to_string()))],
        );
        let error = server
            .read_resource_uri(&nowhere)
            .await
            .expect_err("a name defined nowhere is a miss");
        assert_eq!(error.code, rmcp::model::ErrorCode::RESOURCE_NOT_FOUND);

        let tool = server.get_symbol_context(Parameters(input("main"))).await;
        assert_eq!(
            classify_compact_tool_output("get_symbol_context", &tool),
            OutcomeClass::Found,
            "{tool}"
        );
        let existing = build_uri(
            "symforge://symbol/context",
            &[("name", Some("main".to_string()))],
        );
        let result = server
            .read_resource_uri(&existing)
            .await
            .expect("an existing unreferenced symbol is a successful read");
        let text = match &result.contents[0] {
            ResourceContents::TextResourceContents { text, .. } => text,
            other => panic!("expected text resource, got {other:?}"),
        };
        assert!(text.contains("No references found"), "{text}");
    }

    /// A JSON-RPC client over an in-process duplex transport, so reads go
    /// through the real `call_tool` and `read_resource` seams.
    struct WireClient {
        lines: tokio::io::Lines<tokio::io::BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>>,
        writer: tokio::io::WriteHalf<tokio::io::DuplexStream>,
        next_id: u64,
    }

    impl WireClient {
        async fn connect(server: SymForgeServer) -> Self {
            use tokio::io::AsyncBufReadExt;
            let (client, transport) = tokio::io::duplex(1 << 20);
            tokio::spawn(async move {
                let service = rmcp::serve_server(server, tokio::io::split(transport)).await?;
                service.waiting().await?;
                anyhow::Ok(())
            });
            let (reader, writer) = tokio::io::split(client);
            let mut wire = Self {
                lines: tokio::io::BufReader::new(reader).lines(),
                writer,
                next_id: 1,
            };
            wire.request(
                "initialize",
                serde_json::json!({
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": {"name": "read-outcome-table", "version": "0"}
                }),
            )
            .await;
            wire.send(serde_json::json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
                .await;
            wire
        }

        async fn send(&mut self, message: serde_json::Value) {
            use tokio::io::AsyncWriteExt;
            let mut line = message.to_string();
            line.push('\n');
            self.writer.write_all(line.as_bytes()).await.unwrap();
        }

        async fn request(&mut self, method: &str, params: serde_json::Value) -> serde_json::Value {
            let id = self.next_id;
            self.next_id += 1;
            self.send(
                serde_json::json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}),
            )
            .await;
            loop {
                let line = tokio::time::timeout(Duration::from_secs(30), self.lines.next_line())
                    .await
                    .expect("reply within 30s")
                    .unwrap()
                    .expect("open transport");
                let message: serde_json::Value = serde_json::from_str(&line).unwrap();
                if message.get("id") == Some(&serde_json::json!(id)) {
                    return message;
                }
            }
        }
    }

    /// Sets `SYMFORGE_SURFACE` for one call and restores it on drop.
    struct SurfaceEnv(Option<std::ffi::OsString>);

    #[allow(unsafe_code)] // test-only env mutation, single-threaded gates.
    impl SurfaceEnv {
        fn set(value: &str) -> Self {
            let previous = std::env::var_os("SYMFORGE_SURFACE");
            // SAFETY: test-only; the gates run tests single-threaded.
            unsafe { std::env::set_var("SYMFORGE_SURFACE", value) };
            Self(previous)
        }
    }

    #[allow(unsafe_code)] // test-only env mutation, single-threaded gates.
    impl Drop for SurfaceEnv {
        fn drop(&mut self) {
            // SAFETY: as in `set`.
            unsafe {
                match self.0.take() {
                    Some(previous) => std::env::set_var("SYMFORGE_SURFACE", previous),
                    None => std::env::remove_var("SYMFORGE_SURFACE"),
                }
            }
        }
    }

    fn resource_class(reply: &serde_json::Value, label: &str) -> OutcomeClass {
        let Some(error) = reply.get("error") else {
            return OutcomeClass::Found;
        };
        let code = error["code"].as_i64().unwrap();
        match code {
            -32002 => OutcomeClass::NotFound,
            -32602 if error["data"]["reason"] == "ambiguous" => OutcomeClass::Ambiguous,
            -32602 => OutcomeClass::InvalidRequest,
            -32603 => OutcomeClass::InternalFailure,
            _ => panic!("{label}: unexpected resource error {reply}"),
        }
    }

    fn tool_class(reply: &serde_json::Value, label: &str) -> (OutcomeClass, bool) {
        let result = &reply["result"];
        let class = serde_json::from_value::<OutcomeClass>(
            result["_meta"]["symforge/result_status"]["outcome_class"].clone(),
        )
        .unwrap_or_else(|_| panic!("{label}: no typed status on {reply}"));
        (class, result["isError"].as_bool().unwrap_or(false))
    }

    /// The shared outcome table, MCP side: each case through `resources/read`
    /// and through `tools/call` on the wire, so the call-seam status exemption
    /// is under test, not only the tool handler.
    #[tokio::test]
    async fn resource_read_outcomes_match_the_shared_table() {
        use crate::protocol::result_status::read_outcome_cases::{CASES, FILES, Read};

        let root = tempfile::tempdir().unwrap();
        for (path, content) in FILES {
            let path = root.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, content).unwrap();
        }
        let loaded = SymForgeServer::new(
            LiveIndex::load(root.path()).expect("index"),
            "read-outcome-table".to_string(),
            Arc::new(Mutex::new(WatcherInfo::default())),
            Some(root.path().to_path_buf()),
            None,
        );
        let unready_index = LiveIndex::empty();
        unready_index.mark_bootstrap_loading();
        let unready = SymForgeServer::new(
            unready_index,
            "read-outcome-table-unready".to_string(),
            Arc::new(Mutex::new(WatcherInfo::default())),
            None,
            None,
        );
        let mut wire = WireClient::connect(loaded.clone()).await;
        let mut unready_wire = WireClient::connect(unready.clone()).await;
        for case in CASES {
            let (uri, tool, arguments) = match &case.read {
                Read::Content {
                    path,
                    raw,
                    around_match,
                    match_occurrence,
                    start_line,
                } => {
                    let mut arguments = serde_json::json!({ "path": path });
                    if let Some(line) = start_line {
                        arguments["start_line"] = serde_json::json!(line);
                    }
                    if let Some(needle) = around_match {
                        arguments["around_match"] = serde_json::json!(needle);
                    }
                    if let Some(count) = match_occurrence {
                        arguments["match_occurrence"] = serde_json::json!(count);
                    }
                    if *raw {
                        arguments["show_line_numbers"] = serde_json::json!(false);
                        arguments["header"] = serde_json::json!(false);
                    }
                    let uri = build_uri(
                        "symforge://file/content",
                        &[
                            ("path", Some(path.to_string())),
                            ("start_line", start_line.map(|line| line.to_string())),
                            ("around_match", around_match.map(str::to_string)),
                            (
                                "match_occurrence",
                                match_occurrence.map(|count| count.to_string()),
                            ),
                            ("show_line_numbers", raw.then(|| "false".to_string())),
                            ("header", raw.then(|| "false".to_string())),
                        ],
                    );
                    (Some(uri), Some("get_file_content"), arguments)
                }
                Read::Context { path } => (
                    Some(build_uri(
                        "symforge://file/context",
                        &[("path", Some(path.to_string()))],
                    )),
                    Some("get_file_context"),
                    serde_json::json!({ "path": path }),
                ),
                Read::Symbol { path, name, kind } => {
                    let mut arguments = serde_json::json!({ "path": path, "name": name });
                    if let Some(kind) = kind {
                        arguments["kind"] = serde_json::json!(kind);
                    }
                    let uri = build_uri(
                        "symforge://symbol/detail",
                        &[
                            ("path", Some(path.to_string())),
                            ("name", Some(name.to_string())),
                            ("kind", kind.map(str::to_string)),
                        ],
                    );
                    (Some(uri), Some("get_symbol"), arguments)
                }
                Read::SymbolContext { name, file } => {
                    let mut arguments = serde_json::json!({ "name": name });
                    if let Some(file) = file {
                        arguments["file"] = serde_json::json!(file);
                    }
                    let uri = build_uri(
                        "symforge://symbol/context",
                        &[
                            ("name", Some(name.to_string())),
                            ("file", file.map(str::to_string)),
                        ],
                    );
                    (Some(uri), Some("get_symbol_context"), arguments)
                }
                Read::Batch { targets } => {
                    let targets: Vec<_> = targets
                        .iter()
                        .map(|(path, name)| serde_json::json!({ "path": path, "name": name }))
                        .collect();
                    (
                        None,
                        Some("get_symbol"),
                        serde_json::json!({ "targets": targets }),
                    )
                }
                Read::ByteSlice {
                    path,
                    start_byte,
                    end_byte,
                    ..
                } => (
                    None,
                    Some("get_symbol"),
                    serde_json::json!({
                        "targets": [{
                            "path": path,
                            "start_byte": start_byte,
                            "end_byte": end_byte,
                        }]
                    }),
                ),
                Read::Bundle { path, name } => (
                    None,
                    Some("get_symbol_context"),
                    serde_json::json!({ "path": path, "name": name, "bundle": true }),
                ),
                Read::Facade { path, symbol } => (
                    None,
                    Some("symforge"),
                    serde_json::json!({
                        "query": "read it",
                        "intent": "read",
                        "path": path,
                        "symbol": symbol,
                    }),
                ),
                Read::Tool { tool, args } => (
                    None,
                    Some(*tool),
                    serde_json::from_str::<serde_json::Value>(args).unwrap(),
                ),
                Read::FacadeQuery { query } => (
                    None,
                    Some("symforge"),
                    serde_json::json!({ "query": query }),
                ),
                Read::Unready => (
                    Some(build_uri(
                        "symforge://file/content",
                        &[("path", Some("src/lib.rs".to_string()))],
                    )),
                    Some("get_file_content"),
                    serde_json::json!({ "path": "src/lib.rs" }),
                ),
            };
            let client = if matches!(case.read, Read::Unready) {
                &mut unready_wire
            } else {
                &mut wire
            };
            if let Some(uri) = uri {
                let reply = client
                    .request("resources/read", serde_json::json!({ "uri": uri }))
                    .await;
                assert_eq!(
                    resource_class(&reply, case.label),
                    case.expected,
                    "resources/read: {}: {reply}",
                    case.label
                );
            }
            if let Some(tool) = tool {
                // The facade answers only on the compact surface, which the
                // server reads from the environment at call time.
                let _compact = matches!(case.read, Read::Facade { .. } | Read::FacadeQuery { .. })
                    .then(|| SurfaceEnv::set("compact"));
                let reply = client
                    .request(
                        "tools/call",
                        serde_json::json!({ "name": tool, "arguments": arguments }),
                    )
                    .await;
                let (class, is_error) = tool_class(&reply, case.label);
                assert_eq!(class, case.expected, "tools/call: {}: {reply}", case.label);
                assert_eq!(
                    is_error,
                    case.expected.is_error(),
                    "tools/call isError: {}: {reply}",
                    case.label
                );
                if let Read::ByteSlice {
                    expected_output, ..
                } = &case.read
                {
                    assert_eq!(
                        reply["result"]["content"][0]["text"].as_str(),
                        Some(*expected_output),
                        "tools/call byte-slice output: {}",
                        case.label
                    );
                }
                // The same tool as a compact-facade step: the step's own
                // typed outcome, on the same server.
                if !matches!(case.read, Read::Facade { .. } | Read::FacadeQuery { .. }) {
                    let server = if matches!(case.read, Read::Unready) {
                        &unready
                    } else {
                        &loaded
                    };
                    let (_, step) = server.facade_step(tool, arguments.clone()).await;
                    assert_eq!(step, case.expected, "facade step: {}", case.label);
                }
            }
        }
    }
}
