use reqwest::Url;
use rmcp::ErrorData as McpError;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{ReadResourceResult, Resource, ResourceContents, ResourceTemplate};

use super::SymForgeServer;
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
        let text = self
            .render_resource_text(request)
            .await
            .map_err(|error| McpError::invalid_params(error, None))?;

        Ok(ReadResourceResult::new(vec![
            ResourceContents::text(text, uri.to_string()).with_mime_type("text/markdown"),
        ]))
    }

    async fn render_resource_text(&self, request: ResourceRequest) -> Result<String, String> {
        let text = match request {
            ResourceRequest::RepoHealth => {
                self.health(Parameters(crate::protocol::tools::HealthInput::default()))
                    .await
            }
            ResourceRequest::RepoOutline => {
                self.get_repo_map(Parameters(GetRepoMapInput {
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
            ResourceRequest::Glossary => render_glossary(),
            ResourceRequest::FileContext { path, max_tokens } => {
                self.get_file_context(Parameters(GetFileContextInput {
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

/// Render the `symforge://glossary` resource: a concise legend of the surface
/// vocabulary so callers stop misusing it (e.g. treating the within-project
/// `path:` filter as a project selector). Static markdown; no index access.
fn render_glossary() -> String {
    String::from(
        "SymForge glossary — surface vocabulary.\n\
         \n\
         ## Query parameters (`symforge` tool)\n\
         - `query` (required): a natural-language phrase to route. Omitting it returns a clean\n\
           \"query is required\" error.\n\
         - `intent` (optional hint): orient | find | read | trace | impact | edit | meta | auto.\n\
           Routes to the matching tool family; `auto` infers from the query.\n\
         - `path` (optional): a WITHIN-PROJECT file/dir FILTER. It does NOT select or switch the\n\
           project; a path outside the bound project is rejected. To answer about a different\n\
           project, retarget the connection (declare workspace roots, or `index_folder`).\n\
         - `symbol` (optional): when set, it is the symbol to read/trace; it is honored over\n\
           query-token extraction.\n\
         \n\
         ## Project binding\n\
         - A connection is bound to ONE project, derived from its declared workspace roots (not the\n\
           server's launch directory). Every `status`/`symforge` response surfaces the bound\n\
           `project_root` so a stale/wrong binding is visible, not silent.\n\
         - `base + overlay` index model: an immutable base (shared, keyed by canonical-root + commit)\n\
           plus a per-connection copy-on-write overlay of uncommitted edits; one connection's edits\n\
           are never visible to another.\n\
         \n\
         ## Run modes\n\
         - `stdio` — one MCP process per client over stdin/stdout (the usual editor attach).\n\
         - `daemon` — a shared background process holding warm indexes; stdio front-ends proxy to it.\n\
         - `serve` — an operator HTTP server exposing `/mcp` and `/admin`.\n\
         - `init` — writes MCP attach config into a client; it does not index.\n\
         \n\
         ## Economy figures (trust envelope)\n\
         - `saved`/`predicted`/`net` token numbers are ESTIMATES from heuristics, not measured\n\
           savings. Treat them as indicative, not as a billing-grade measurement.\n\
         \n\
         ## Status fields\n\
         - `index_ready` / `index_files` / `index_symbols`: the bound project's in-memory index.\n\
         - `project_root`: the canonical root currently answering (see Project binding).\n\
         - `deferred`: capabilities present in the surface but not yet wired.\n",
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
        let byte_range = (0, 10);
        let symbol = SymbolRecord {
            name: "main".to_string(),
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
        let mut files = HashMap::new();
        files.insert(path.to_string(), std::sync::Arc::new(file));
        let mut index = LiveIndex {
            files,
            loaded_at: Instant::now(),
            loaded_at_system: std::time::SystemTime::now(),
            load_duration: Duration::from_millis(10),
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
}
