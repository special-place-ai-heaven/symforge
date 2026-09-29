//! The stdio front handler: answers the MCP handshake before the project
//! runtime exists.
//!
//! The stdio adapter used to open its daemon session (or build its local index)
//! before the transport came up, so `initialize` sat unread until the project
//! was loaded. On a large repository that is minutes, far past a harness's
//! connect timeout, and the server was reported failed. This handler goes onto
//! the transport first. Its static surfaces (`initialize`, `discover`, and the
//! tool/resource/prompt lists) come from a shell [`SymForgeServer`], whose list
//! surfaces are the same static code the real server runs. Everything that
//! touches a project goes to the real server once the startup task publishes
//! it.
//!
//! A tool call, resource read or prompt that arrives while the project is
//! still opening, or a tool call while its index is still loading, waits here
//! for up to [`TOOL_READINESS_WAIT`] so small and medium repositories never
//! surface the condition. Past that it is answered with the initial-indexing
//! notice, typed as not executed, never with an empty answer. `status` and
//! `health` never wait: they report the loading state at once.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, GetPromptRequestParams,
    GetPromptResponse, ListPromptsResult, ListResourceTemplatesResult, ListResourcesResult,
    ListToolsResult, PaginatedRequestParams, ProgressNotificationParam, ProtocolVersion,
    ReadResourceRequestParams, ReadResourceResponse, ServerInfo,
};
use rmcp::service::{NotificationContext, RequestContext};
use rmcp::{ErrorData, RoleServer, ServerHandler};
use tokio::sync::watch;

use super::SymForgeServer;
use super::format::INITIAL_INDEXING_IN_PROGRESS;
use super::result_status::{
    OutcomeClass, PROJECT_EVIDENCE_META_KEY, ResultStatus, attach_project_evidence_meta,
};

/// How long a request waits server-side for the project to become ready
/// before answering not-ready. Kept below common harness tool timeouts.
pub const TOOL_READINESS_WAIT: Duration = Duration::from_secs(25);
/// How often a waiting call re-checks readiness when nothing was published.
const READINESS_POLL: Duration = Duration::from_millis(500);
/// How often a waiting call that carried a progress token reports progress.
const PROGRESS_INTERVAL: Duration = Duration::from_secs(5);

/// How far the stdio startup task has got.
#[derive(Clone)]
pub enum StdioStartup {
    /// The project runtime is still being opened or built.
    Starting,
    /// The runtime exists; every request goes to it.
    Ready(Arc<SymForgeServer>),
    /// Startup failed; nothing will become ready in this process.
    Failed(Arc<str>),
}

/// Held by the startup task. If the task ends while the front still says
/// `Starting`, by a panic or by an exit that published nothing, dropping this
/// publishes a failure, so waiting requests end instead of waiting for a
/// runtime that is never coming.
pub struct StartupGuard(pub Arc<watch::Sender<StdioStartup>>);

impl Drop for StartupGuard {
    fn drop(&mut self) {
        self.0.send_if_modified(|state| {
            let starting = matches!(state, StdioStartup::Starting);
            if starting {
                *state = StdioStartup::Failed(
                    "the startup task stopped before the project runtime came up. This is a \
                     symforge bug; a panic message, if there was one, is on symforge's stderr"
                        .into(),
                );
            }
            starting
        });
    }
}

#[derive(Clone)]
pub struct DeferredStdioServer {
    /// Serves only the static surfaces. It is never bound to a project and
    /// never runs a tool.
    shell: Arc<SymForgeServer>,
    project_root: Arc<str>,
    project_name: Arc<str>,
    /// When this process began opening the project: the only clock the
    /// not-ready answers report, because it is the only one observed here.
    opened_at: Instant,
    readiness_wait: Duration,
    startup: watch::Receiver<StdioStartup>,
}

impl DeferredStdioServer {
    /// A front handler for `project_root`, plus the sender the startup task
    /// publishes the real server (or its failure) through.
    pub fn new(project_root: &Path) -> (Self, watch::Sender<StdioStartup>) {
        let (sender, startup) = watch::channel(StdioStartup::Starting);
        let shell = SymForgeServer::new(
            crate::live_index::LiveIndex::empty(),
            "project".to_string(),
            Arc::new(Mutex::new(crate::watcher::WatcherInfo::default())),
            None,
            None,
        );
        let project_name = project_root
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| project_root.display().to_string());
        let front = Self {
            shell: Arc::new(shell),
            project_root: project_root.display().to_string().into(),
            project_name: project_name.into(),
            opened_at: Instant::now(),
            readiness_wait: TOOL_READINESS_WAIT,
            startup,
        };
        (front, sender)
    }

    #[cfg(test)]
    fn with_readiness_wait(mut self, wait: Duration) -> Self {
        self.readiness_wait = wait;
        self
    }

    /// The initial-indexing notice with the one progress figure observed here.
    fn indexing_line(&self) -> String {
        format!(
            "symforge: {}: {INITIAL_INDEXING_IN_PROGRESS}. {}s since this symforge process began \
             opening it; tool calls wait up to {}s for it.",
            self.project_name,
            self.opened_at.elapsed().as_secs(),
            self.readiness_wait.as_secs()
        )
    }

    fn not_ready_text(&self, waited: Duration) -> String {
        format!(
            "Index is loading... {} This call waited {}s and was not executed. Nothing is wrong: \
             retry the same call, and do not read this as an error or an empty result.",
            self.indexing_line(),
            waited.as_secs()
        )
    }

    fn failed_text(&self, reason: &str) -> String {
        format!(
            "Error: symforge failed to start for {}: {reason}",
            self.project_root
        )
    }

    fn not_ready_result(&self, waited: Duration) -> CallToolResponse {
        front_answer(
            ResultStatus::new(OutcomeClass::InternalFailure)
                .into_call_tool_result(self.not_ready_text(waited)),
        )
    }

    /// The real server for a resource read or prompt, waiting for it as a
    /// tool call does. Still starting after the wait is an explicit,
    /// retryable not-ready error.
    async fn ready_server(&self) -> Result<Arc<SymForgeServer>, ErrorData> {
        let started = Instant::now();
        let mut startup = self.startup.clone();
        let _ = tokio::time::timeout(
            self.readiness_wait,
            startup.wait_for(|state| !matches!(state, StdioStartup::Starting)),
        )
        .await;
        let phase = self.startup.borrow().clone();
        match phase {
            StdioStartup::Ready(server) => Ok(server),
            StdioStartup::Failed(reason) => {
                Err(ErrorData::internal_error(self.failed_text(&reason), None))
            }
            StdioStartup::Starting => {
                let waited = started.elapsed();
                Err(ErrorData::internal_error(
                    self.not_ready_text(waited),
                    Some(serde_json::json!({
                        "retryable": true,
                        "waited_ms": waited.as_millis() as u64,
                    })),
                ))
            }
        }
    }
}

/// An answer the front built itself. No project is bound behind it yet, so it
/// carries the explicit evidence-unavailable marker every tool result must
/// carry (FR-319) rather than no evidence at all.
fn front_answer(mut result: CallToolResult) -> CallToolResponse {
    attach_project_evidence_meta(&mut result.meta);
    CallToolResponse::Complete(result)
}

/// A loading-guard refusal. Every guard returns it before its tool body
/// runs, so the call can be repeated without repeating any effect.
fn is_loading_refusal(response: &CallToolResponse) -> bool {
    let CallToolResponse::Complete(result) = response else {
        return false;
    };
    result
        .content
        .first()
        .and_then(|block| block.as_text())
        .is_some_and(|text| text.text.starts_with("Index is loading"))
}

/// Whether the project evidence the server attached observed a loading index.
fn reports_loading(result: &CallToolResult) -> bool {
    result
        .meta
        .as_ref()
        .and_then(|meta| meta.0.get(PROJECT_EVIDENCE_META_KEY))
        .and_then(|evidence| evidence.get("index_state"))
        .and_then(|state| state.as_str())
        == Some("Loading")
}

impl ServerHandler for DeferredStdioServer {
    fn get_info(&self) -> ServerInfo {
        self.shell.get_info()
    }

    fn supported_protocol_versions(&self) -> std::borrow::Cow<'static, [ProtocolVersion]> {
        self.shell.supported_protocol_versions()
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        self.shell.list_tools(request, context).await
    }

    async fn list_resources(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        self.shell.list_resources(request, context).await
    }

    async fn list_resource_templates(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, ErrorData> {
        self.shell.list_resource_templates(request, context).await
    }

    async fn list_prompts(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, ErrorData> {
        self.shell.list_prompts(request, context).await
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        // An off-surface or unknown tool is refused the way the real server
        // refuses it, ready or not: its router is the shell's.
        super::surface_probe::enforce_compact_surface(request.name.as_ref())?;
        if !self.shell.tool_router.has_route(request.name.as_ref()) {
            return Err(ErrorData::invalid_params("tool not found", None));
        }
        let diagnostic = matches!(
            request.name.as_ref(),
            "status" | "health" | "health_compact"
        );
        let started = Instant::now();
        let progress_token = context.meta.get_progress_token();
        let mut next_progress = started;
        let mut startup = self.startup.clone();
        loop {
            let phase = startup.borrow_and_update().clone();
            match phase {
                StdioStartup::Failed(reason) => {
                    return Ok(front_answer(
                        ResultStatus::new(OutcomeClass::InternalFailure)
                            .into_call_tool_result(self.failed_text(&reason)),
                    ));
                }
                StdioStartup::Starting if diagnostic => {
                    let text = format!(
                        "{}\nProject root: {}\nIf a tool call still reports loading after its \
                         wait, retry the same call.",
                        self.indexing_line(),
                        self.project_root,
                    );
                    return Ok(front_answer(CallToolResult::success(vec![
                        ContentBlock::text(text),
                    ])));
                }
                StdioStartup::Starting => {}
                StdioStartup::Ready(server) => {
                    let mut response = server.call_tool(request.clone(), context.clone()).await?;
                    if diagnostic || !is_loading_refusal(&response) {
                        if diagnostic
                            && let CallToolResponse::Complete(result) = &mut response
                            && reports_loading(result)
                        {
                            result
                                .content
                                .insert(0, ContentBlock::text(self.indexing_line()));
                        }
                        return Ok(response);
                    }
                }
            }

            let waited = started.elapsed();
            if waited >= self.readiness_wait {
                return Ok(self.not_ready_result(waited));
            }
            if let Some(token) = &progress_token
                && Instant::now() >= next_progress
            {
                let progress = ProgressNotificationParam::new(token.clone(), waited.as_secs_f64())
                    .with_message(self.indexing_line());
                let _ = context.peer.notify_progress(progress).await;
                next_progress = Instant::now() + PROGRESS_INTERVAL;
            }
            let pause = READINESS_POLL.min(self.readiness_wait - waited);
            tokio::select! {
                _ = context.ct.cancelled() => return Ok(self.not_ready_result(started.elapsed())),
                changed = startup.changed() => {
                    // A dropped sender publishes nothing more; keep polling.
                    if changed.is_err() {
                        tokio::time::sleep(pause).await;
                    }
                }
                _ = tokio::time::sleep(pause) => {}
            }
        }
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        self.ready_server()
            .await?
            .read_resource(request, context)
            .await
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResponse, ErrorData> {
        self.ready_server()
            .await?
            .get_prompt(request, context)
            .await
    }

    /// The real server binds client-declared roots here, and it may not exist
    /// yet: replay the notification to it once it does.
    async fn on_initialized(&self, context: NotificationContext<RoleServer>) {
        let mut startup = self.startup.clone();
        tokio::spawn(async move {
            let server = match startup
                .wait_for(|state| !matches!(state, StdioStartup::Starting))
                .await
                .as_deref()
            {
                Ok(StdioStartup::Ready(server)) => Arc::clone(server),
                _ => return,
            };
            server.on_initialized(context).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    struct Client {
        lines: tokio::io::Lines<BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>>,
        writer: tokio::io::WriteHalf<tokio::io::DuplexStream>,
        /// Notifications that arrived while waiting for a reply.
        notifications: Vec<Value>,
    }

    impl Client {
        async fn send(&mut self, message: Value) {
            let mut line = message.to_string();
            line.push('\n');
            self.writer.write_all(line.as_bytes()).await.unwrap();
        }

        async fn request(&mut self, id: u64, method: &str, params: Value) -> Value {
            self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
                .await;
            loop {
                let line = tokio::time::timeout(
                    std::time::Duration::from_secs(10),
                    self.lines.next_line(),
                )
                .await
                .expect("the front handler answers within its readiness wait")
                .unwrap()
                .expect("an open transport");
                let message: Value = serde_json::from_str(&line).unwrap();
                if message.get("id").is_none() {
                    self.notifications.push(message);
                    continue;
                }
                assert_eq!(
                    message["id"],
                    json!(id),
                    "reply to the request just sent: {message}"
                );
                return message;
            }
        }

        async fn initialize(&mut self) {
            let init = self
                .request(
                    1,
                    "initialize",
                    json!({
                        "protocolVersion": "2025-06-18",
                        "capabilities": {},
                        "clientInfo": {"name": "deferred-stdio-test", "version": "0"}
                    }),
                )
                .await;
            assert_eq!(init["result"]["serverInfo"]["name"], json!("symforge"));
            self.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
                .await;
        }

        async fn call(&mut self, id: u64, name: &str, meta: Option<Value>) -> Value {
            let mut params = json!({"name": name, "arguments": {}});
            if let Some(meta) = meta {
                params["_meta"] = meta;
            }
            self.request(id, "tools/call", params).await
        }
    }

    fn connect<H: ServerHandler>(handler: H) -> Client {
        let (client, server) = tokio::io::duplex(1 << 20);
        tokio::spawn(async move {
            let service = rmcp::serve_server(handler, tokio::io::split(server)).await?;
            service.waiting().await?;
            anyhow::Ok(())
        });
        let (reader, writer) = tokio::io::split(client);
        Client {
            lines: BufReader::new(reader).lines(),
            writer,
            notifications: Vec::new(),
        }
    }

    fn tool_text(reply: &Value) -> &str {
        reply["result"]["content"][0]["text"].as_str().unwrap()
    }

    fn outcome_class(reply: &Value) -> &Value {
        &reply["result"]["_meta"]["symforge/result_status"]["outcome_class"]
    }

    fn server_with(index: crate::live_index::SharedIndex) -> Arc<SymForgeServer> {
        Arc::new(SymForgeServer::new(
            index,
            "project".to_string(),
            Arc::new(Mutex::new(crate::watcher::WatcherInfo::default())),
            None,
            None,
        ))
    }

    fn loading_server() -> Arc<SymForgeServer> {
        let index = crate::live_index::LiveIndex::empty();
        index.mark_bootstrap_loading();
        server_with(index)
    }

    /// The regression: `initialize` and the list surfaces must not wait on the
    /// project runtime. A tool call that outlives its readiness wait gets the
    /// initial-indexing notice with the machine-readable outcome, not an empty
    /// answer; `status` answers at once with the same notice.
    #[tokio::test]
    async fn handshake_is_answered_while_the_project_runtime_is_still_starting() {
        let (front, _startup) = DeferredStdioServer::new(Path::new("/work/large-repo"));
        let mut client = connect(front.with_readiness_wait(Duration::from_millis(300)));
        client.initialize().await;

        let tools = client.request(2, "tools/list", json!({})).await;
        assert!(
            !tools["result"]["tools"].as_array().unwrap().is_empty(),
            "tools/list is served from the static surface: {tools}"
        );

        let pending = client.call(3, "search_symbols", None).await;
        assert_eq!(pending["result"]["isError"], json!(true), "{pending}");
        assert_eq!(
            outcome_class(&pending),
            &json!("internal_failure"),
            "{pending}"
        );
        let text = tool_text(&pending);
        assert!(text.starts_with("Index is loading"), "{text}");
        assert!(text.contains(INITIAL_INDEXING_IN_PROGRESS), "{text}");
        assert!(text.contains("large-repo"), "names the project: {text}");
        assert!(text.contains("retry the same call"), "{text}");
        assert_eq!(
            pending["result"]["_meta"]["symforge/project_evidence"]["bound"],
            json!(false),
            "a front answer discloses that no project evidence exists yet: {pending}"
        );

        let status = client.call(4, "status", None).await;
        assert_ne!(status["result"]["isError"], json!(true), "{status}");
        assert!(
            tool_text(&status).contains(INITIAL_INDEXING_IN_PROGRESS),
            "{status}"
        );
        assert_eq!(
            status["result"]["_meta"]["symforge/project_evidence"]["bound"],
            json!(false),
            "{status}"
        );
    }

    /// A call that arrives before the runtime exists waits for it and is then
    /// served by it, so a quick startup never surfaces the condition.
    #[tokio::test]
    async fn a_tool_call_waits_for_the_runtime_and_is_then_served() {
        let (front, startup) = DeferredStdioServer::new(Path::new("/work/medium-repo"));
        let mut client = connect(front.with_readiness_wait(Duration::from_secs(8)));
        client.initialize().await;

        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            // An empty, not-loading index answers at once with its own guard.
            startup.send_replace(StdioStartup::Ready(server_with(
                crate::live_index::LiveIndex::empty(),
            )));
        });
        let served = client
            .call(2, "search_symbols", Some(json!({"progressToken": "tok-1"})))
            .await;
        assert!(
            !tool_text(&served).contains(INITIAL_INDEXING_IN_PROGRESS),
            "the published server answered after the wait: {served}"
        );
        assert!(
            client.notifications.iter().any(|note| {
                note["method"] == json!("notifications/progress")
                    && note["params"]["progressToken"] == json!("tok-1")
                    && note["params"]["message"]
                        .as_str()
                        .is_some_and(|message| message.contains(INITIAL_INDEXING_IN_PROGRESS))
            }),
            "a waiting call with a progress token reports progress: {:?}",
            client.notifications
        );
    }

    /// Once the runtime exists but its index is still loading, a tool keeps
    /// waiting and then gets the same notice; `health` answers at once and
    /// leads with the notice; a plain-String guard refusal is typed.
    #[tokio::test]
    async fn a_loading_index_behind_a_ready_runtime_gets_the_same_notice() {
        let (front, startup) = DeferredStdioServer::new(Path::new("/work/large-repo"));
        startup.send_replace(StdioStartup::Ready(loading_server()));
        let mut client = connect(front.with_readiness_wait(Duration::from_millis(600)));
        client.initialize().await;

        let loading = client.call(2, "get_repo_map", None).await;
        assert_eq!(loading["result"]["isError"], json!(true), "{loading}");
        assert_eq!(
            outcome_class(&loading),
            &json!("internal_failure"),
            "{loading}"
        );
        assert!(
            tool_text(&loading).contains(INITIAL_INDEXING_IN_PROGRESS),
            "{loading}"
        );

        let health = client.call(3, "health", None).await;
        assert!(
            tool_text(&health).contains(INITIAL_INDEXING_IN_PROGRESS),
            "health leads with the notice while the index loads: {health}"
        );
    }

    #[tokio::test]
    async fn a_failed_startup_is_reported_instead_of_loading_forever() {
        let (front, startup) = DeferredStdioServer::new(Path::new("/work/large-repo"));
        let mut client = connect(front.with_readiness_wait(Duration::from_secs(8)));
        client.initialize().await;

        // Fails while the call is already waiting: the wait ends at once.
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            startup.send_replace(StdioStartup::Failed("daemon unreachable".into()));
        });
        let started = Instant::now();
        let reply = client.call(2, "search_symbols", None).await;
        assert!(started.elapsed() < Duration::from_secs(5), "no full wait");
        assert_eq!(reply["result"]["isError"], json!(true), "{reply}");
        assert!(
            tool_text(&reply).starts_with("Error: symforge failed to start")
                && tool_text(&reply).contains("daemon unreachable"),
            "{reply}"
        );

        let status = client.call(3, "status", None).await;
        assert!(
            tool_text(&status).contains("daemon unreachable"),
            "{status}"
        );
    }

    fn real_server() -> SymForgeServer {
        SymForgeServer::new(
            crate::live_index::LiveIndex::empty(),
            "project".to_string(),
            Arc::new(Mutex::new(crate::watcher::WatcherInfo::default())),
            None,
            None,
        )
    }

    /// Everything the front answers from its shell is what the real server
    /// answers: the four lists, and the error for a tool no router has, which
    /// must not be mistaken for a tool that is still loading.
    #[tokio::test]
    async fn the_front_answers_its_static_surfaces_as_the_real_server_does() {
        let (front, _startup) = DeferredStdioServer::new(Path::new("/work/large-repo"));
        let mut front = connect(front.with_readiness_wait(Duration::from_millis(300)));
        let mut real = connect(real_server());
        front.initialize().await;
        real.initialize().await;

        for (id, method) in [
            (2, "tools/list"),
            (3, "resources/list"),
            (4, "resources/templates/list"),
            (5, "prompts/list"),
        ] {
            let ours = front.request(id, method, json!({})).await;
            let theirs = real.request(id, method, json!({})).await;
            assert!(ours.get("result").is_some(), "{method}: {ours}");
            assert_eq!(ours, theirs, "{method}");
        }

        let ours = front.call(6, "no_such_tool", None).await;
        let theirs = real.call(6, "no_such_tool", None).await;
        assert!(
            ours.get("error").is_some(),
            "an unknown tool is an error, not a loading notice: {ours}"
        );
        assert_eq!(ours, theirs);
    }

    /// A resource read waits like a tool call, then says explicitly that it
    /// was not served and can be retried; once the runtime exists it is
    /// served by it.
    #[tokio::test]
    async fn a_resource_read_while_starting_waits_then_is_explicitly_retryable() {
        let (front, startup) = DeferredStdioServer::new(Path::new("/work/large-repo"));
        let mut client = connect(front.with_readiness_wait(Duration::from_millis(300)));
        client.initialize().await;
        let read = json!({"uri": "symforge://repo/health"});

        let started = Instant::now();
        let pending = client.request(2, "resources/read", read.clone()).await;
        assert!(started.elapsed() >= Duration::from_millis(300), "it waited");
        let message = pending["error"]["message"].as_str().unwrap_or_default();
        assert!(
            message.contains(INITIAL_INDEXING_IN_PROGRESS)
                && message.contains("retry the same call"),
            "{pending}"
        );
        assert_eq!(
            pending["error"]["data"]["retryable"],
            json!(true),
            "{pending}"
        );

        startup.send_replace(StdioStartup::Ready(server_with(
            crate::live_index::LiveIndex::empty(),
        )));
        let served = client.request(3, "resources/read", read).await;
        assert!(
            !served.to_string().contains(INITIAL_INDEXING_IN_PROGRESS),
            "the runtime answered: {served}"
        );
    }

    /// The client's `initialized` reaches the runtime even though it arrived
    /// before the runtime existed: an unbound server asks for the client's
    /// roots from it.
    #[tokio::test]
    async fn initialized_is_replayed_to_the_runtime_once_it_exists() {
        let (front, startup) = DeferredStdioServer::new(Path::new("/work/large-repo"));
        let mut client = connect(front);
        let init = client
            .request(
                1,
                "initialize",
                json!({
                    "protocolVersion": "2025-06-18",
                    "capabilities": {"roots": {"listChanged": true}},
                    "clientInfo": {"name": "deferred-stdio-test", "version": "0"}
                }),
            )
            .await;
        assert!(init.get("result").is_some(), "{init}");
        client
            .send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
            .await;
        tokio::time::sleep(Duration::from_millis(200)).await;

        startup.send_replace(StdioStartup::Ready(server_with(
            crate::live_index::LiveIndex::empty(),
        )));
        let asked = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let line = client.lines.next_line().await.unwrap().expect("open");
                let message: Value = serde_json::from_str(&line).unwrap();
                if message["method"] == json!("roots/list") {
                    return message;
                }
            }
        })
        .await
        .expect("the runtime ran its initialized handler");
        assert!(asked.get("id").is_some(), "{asked}");
    }

    /// A startup task that panics must not leave every request waiting on a
    /// runtime that is never coming.
    #[tokio::test]
    async fn a_startup_task_that_panics_is_reported_as_failed() {
        let (front, startup) = DeferredStdioServer::new(Path::new("/work/large-repo"));
        let guard = StartupGuard(Arc::new(startup));
        let task = tokio::spawn(async move {
            let _guard = guard;
            panic!("startup exploded");
        });
        assert!(task.await.expect_err("the task panicked").is_panic());

        let mut client = connect(front.with_readiness_wait(Duration::from_secs(8)));
        client.initialize().await;
        let started = Instant::now();
        let reply = client.call(2, "search_symbols", None).await;
        assert!(started.elapsed() < Duration::from_secs(5), "no full wait");
        let text = tool_text(&reply);
        assert!(
            text.starts_with("Error: symforge failed to start")
                && text.contains("startup task stopped"),
            "{reply}"
        );
    }
}
