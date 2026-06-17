use parking_lot::Mutex;
use std::sync::Arc;

use clap::Parser;
use rmcp::{serve_server, transport};
use std::ffi::OsString;
use symforge::live_index::persist;
use symforge::{
    cli, daemon, discovery, live_index, observability, protocol, server, sidecar, version_registry,
    watcher,
};

#[derive(Debug, Clone, PartialEq, Eq)]
enum StartupPlan {
    Daemon { root: std::path::PathBuf },
    LocalAutoIndex { root: std::path::PathBuf },
    LocalEmpty { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum StartupIndexLogView {
    Ready {
        file_count: usize,
        symbol_count: usize,
        parsed_count: usize,
        partial_parse_count: usize,
        failed_count: usize,
        duration_ms: u64,
    },
    Degraded {
        summary: String,
    },
}

fn startup_index_log_view(
    published: &live_index::PublishedIndexState,
) -> Option<StartupIndexLogView> {
    match published.status {
        live_index::PublishedIndexStatus::Ready => Some(StartupIndexLogView::Ready {
            file_count: published.file_count,
            symbol_count: published.symbol_count,
            parsed_count: published.parsed_count,
            partial_parse_count: published.partial_parse_count,
            failed_count: published.failed_count,
            duration_ms: published.load_duration.as_millis() as u64,
        }),
        live_index::PublishedIndexStatus::Degraded => Some(StartupIndexLogView::Degraded {
            summary: published
                .degraded_summary
                .clone()
                .unwrap_or_else(|| "circuit breaker tripped".to_string()),
        }),
        live_index::PublishedIndexStatus::Empty | live_index::PublishedIndexStatus::Loading => None,
    }
}

fn local_empty_reason(should_auto_index: bool) -> &'static str {
    if !should_auto_index {
        "SYMFORGE_AUTO_INDEX=false — starting with empty index"
    } else {
        "no safe project root found — starting with empty index"
    }
}

fn startup_plan(
    should_auto_index: bool,
    resolved_root: Option<std::path::PathBuf>,
    daemon_available: bool,
) -> StartupPlan {
    match (resolved_root, daemon_available) {
        (Some(root), true) => StartupPlan::Daemon { root },
        (Some(root), false) => StartupPlan::LocalAutoIndex { root },
        (None, _) => StartupPlan::LocalEmpty {
            reason: local_empty_reason(should_auto_index).to_string(),
        },
    }
}

fn checkpoint_interval_from_value(value: Option<&str>) -> Option<std::time::Duration> {
    persist::checkpoint_interval_from_value(value)
}

fn checkpoint_interval_from_env() -> Option<std::time::Duration> {
    checkpoint_interval_from_value(
        std::env::var(persist::CHECKPOINT_INTERVAL_ENV)
            .ok()
            .as_deref(),
    )
}

fn spawn_periodic_checkpoint(
    index: live_index::SharedIndex,
    root: std::path::PathBuf,
    interval: std::time::Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(interval).await;
            let checkpoint_index = Arc::clone(&index);
            let checkpoint_root = root.clone();
            match tokio::task::spawn_blocking(move || {
                persist::checkpoint_shared_index(&checkpoint_index, &checkpoint_root)
            })
            .await
            {
                Ok(Ok(report)) => tracing::info!(
                    bytes = report.bytes,
                    files = report.files,
                    path = %report.path.display(),
                    "periodic checkpoint wrote .symforge/index.bin"
                ),
                Ok(Err(error)) => tracing::warn!(
                    interval_secs = interval.as_secs(),
                    "periodic checkpoint failed: {error}"
                ),
                Err(join_error) => tracing::warn!(
                    interval_secs = interval.as_secs(),
                    "periodic checkpoint task panicked: {join_error}"
                ),
            }
        }
    })
}

fn main() -> anyhow::Result<()> {
    let args: Vec<OsString> = std::env::args_os().collect();

    // Record this binary's path+version so a stale durable binary can be
    // detected later (see `version_registry`). Best-effort and read-mostly;
    // runs before the `--version` fast path so the npm-installed binary
    // registers itself when the user verifies an update with `symforge --version`.
    version_registry::record_self_default();

    if cli::version::is_version_request(&args) {
        return cli::version::run_version();
    }

    let cli = cli::Cli::parse_from(args);
    match cli.command {
        Some(cli::Commands::Analytics { command }) => cli::analytics::run_analytics(&command),
        Some(cli::Commands::Init {
            client,
            scan,
            apply,
            serve_url,
            serve_key,
        }) => {
            if scan {
                cli::init::run_scan(apply, serve_url, serve_key)
            } else {
                cli::init::run_init(client)
            }
        }
        Some(cli::Commands::Daemon) => run_daemon(),
        Some(cli::Commands::Serve(args)) => run_serve(args),
        Some(cli::Commands::Hook { subcommand }) => cli::hook::run_hook(subcommand.as_ref()),
        Some(cli::Commands::Trust { subcommand }) => cli::trust::run_trust(&subcommand),
        Some(cli::Commands::Update) => cli::update::run_update(),
        None => run_mcp_server(),
    }
}

fn run_daemon() -> anyhow::Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        // Default worker_threads = num_cpus. Tool handlers that acquire
        // std::sync::RwLock are wrapped in spawn_blocking (see daemon.rs
        // call_tool_handler), so they run on the blocking thread pool
        // (up to 512 threads) and don't starve async workers.
        .build()?
        .block_on(async {
            observability::init_tracing()?;
            daemon::run_daemon_until_shutdown("127.0.0.1").await
        })
}

fn run_serve(args: cli::serve::ServeCliArgs) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let result = runtime.block_on(async {
        observability::init_tracing()?;
        // Secure-startup checks (key resolution, loopback, refuse-to-start) run
        // inside `server::serve::run` before any bind; on a permitted config it
        // mounts `/mcp` and runs until shutdown. Map only the tracing-init error
        // to anyhow here; the serve result stays a typed `ServeError` so
        // refuse-to-start can map to exit code 2 below.
        Ok::<Result<(), server::serve::ServeError>, anyhow::Error>(
            server::serve::run(args.into_serve_args()).await,
        )
    })?;

    match result {
        Ok(()) => Ok(()),
        // Secure-default refuse-to-start is exit code 2 (cli-serve contract):
        // distinct from a generic failure so operators/CI can detect a refused
        // bind specifically. Print the cause, then exit 2 directly.
        Err(server::serve::ServeError::Startup(err)) => {
            eprintln!("error: {err}");
            std::process::exit(2);
        }
        Err(other) => Err(anyhow::Error::from(other)),
    }
}

fn run_mcp_server() -> anyhow::Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async { run_mcp_server_async().await })
}

async fn run_mcp_server_async() -> anyhow::Result<()> {
    observability::init_tracing()?;

    // INFR-02: Auto-index on startup (configurable via SYMFORGE_AUTO_INDEX)
    let should_auto_index = std::env::var("SYMFORGE_AUTO_INDEX")
        .map(|v| v != "false")
        .unwrap_or(true);

    let resolved_root = if should_auto_index {
        discovery::find_project_root()
    } else {
        None
    };

    let use_daemon = std::env::var("SYMFORGE_NO_DAEMON")
        .map(|v| v == "0" || v.is_empty())
        .unwrap_or(true);

    if use_daemon && let Some(root) = resolved_root.clone() {
        match daemon::connect_or_spawn_session(&root, "mcp-stdio", Some(std::process::id())).await {
            Ok(session) => return run_remote_mcp_server_async(session).await,
            Err(error) => {
                tracing::warn!(
                    root = %root.display(),
                    "daemon-backed startup failed, falling back to local mode: {error}"
                );
            }
        }
    }

    match startup_plan(should_auto_index, resolved_root, false) {
        StartupPlan::Daemon { .. } => unreachable!("daemon sessions return before local startup"),
        StartupPlan::LocalAutoIndex { root } => {
            run_local_mcp_server_async(should_auto_index, Some(root)).await
        }
        StartupPlan::LocalEmpty { .. } => run_local_mcp_server_async(should_auto_index, None).await,
    }
}

async fn run_remote_mcp_server_async(session: daemon::DaemonSessionClient) -> anyhow::Result<()> {
    if let Some(port) = session.port() {
        sidecar::port_file::write_port_file(port)?;
        sidecar::port_file::write_pid_file(std::process::id())?;
        sidecar::port_file::write_session_file(session.session_id())?;
    }

    let heartbeat_client = session.clone();
    let heartbeat_task = tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(15)).await;
            let _ = heartbeat_client.heartbeat().await;
        }
    });

    let server = protocol::SymForgeServer::new_daemon_proxy(session.clone());
    tracing::info!(
        project_id = %session.project_id(),
        session_id = %session.session_id(),
        "starting daemon-backed MCP server on stdio transport"
    );
    let service = serve_server(server, transport::stdio()).await?;

    tokio::select! {
        result = service.waiting() => { result?; }
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Ctrl+C received, shutting down");
        }
    }

    heartbeat_task.abort();
    let _ = session.close().await;
    sidecar::port_file::cleanup_files();
    tracing::info!("daemon-backed MCP server shut down cleanly");
    Ok(())
}

async fn run_local_mcp_server_async(
    should_auto_index: bool,
    resolved_root: Option<std::path::PathBuf>,
) -> anyhow::Result<()> {
    let (index, project_name, watcher_root) = if let Some(root) = resolved_root {
        tracing::info!(root = %root.display(), "auto-indexing from project root");

        // Try loading from persisted snapshot first (fast path: no re-parsing).
        let index = if let Some(snapshot) = persist::load_snapshot(&root) {
            let file_count = snapshot.files.len();
            // Extract mtime map before consuming snapshot
            let snapshot_mtimes: std::collections::HashMap<String, u64> = snapshot
                .files
                .iter()
                .map(|(k, v)| (k.clone(), v.mtime_secs))
                .collect();

            let live = persist::snapshot_to_live_index(snapshot, &root);
            tracing::info!(
                files = file_count,
                load_source = ?live.load_source(),
                snapshot_verify_state = ?live.snapshot_verify_state(),
                "loaded serialized index from .symforge/index.bin"
            );
            let shared: live_index::SharedIndex = live_index::SharedIndexHandle::shared(live);

            // Spawn background verification to reconcile against current disk state.
            let bg_index = shared.clone();
            let bg_root = root.clone();
            tokio::spawn(async move {
                persist::background_verify(bg_index, bg_root, snapshot_mtimes).await;
            });

            shared
        } else {
            // No snapshot — start with empty index and re-index in background
            // so the MCP server can respond to initialize/tools/list immediately.
            let shared = live_index::LiveIndex::empty();
            let bg_index = shared.clone();
            let bg_root = root.clone();
            tokio::task::spawn_blocking(move || {
                tracing::info!("cold-start indexing in background");
                if let Err(e) = bg_index.reload(&bg_root) {
                    tracing::error!(%e, "background cold-start indexing failed");
                } else {
                    tracing::info!("background cold-start indexing complete");
                }
            });
            shared
        };

        let published = index.published_state();
        match startup_index_log_view(&published) {
            Some(StartupIndexLogView::Ready {
                file_count,
                symbol_count,
                parsed_count,
                partial_parse_count,
                failed_count,
                duration_ms,
            }) => {
                tracing::info!(
                    files = file_count,
                    symbols = symbol_count,
                    parsed = parsed_count,
                    partial = partial_parse_count,
                    failed = failed_count,
                    duration_ms,
                    "LiveIndex ready"
                );
            }
            Some(StartupIndexLogView::Degraded { summary }) => {
                tracing::error!(%summary, "circuit breaker tripped — index degraded");
            }
            None => {}
        }

        let name = root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project")
            .to_string();

        (index, name, Some(root))
    } else {
        tracing::info!("{}", local_empty_reason(should_auto_index));
        let live = live_index::LiveIndex::empty();
        live.set_local_empty_reason(Some(local_empty_reason(should_auto_index).to_string()));
        (live, "project".to_string(), None)
    };

    // Spawn file watcher after initial load (only when auto-index is enabled).
    let watcher_info = Arc::new(Mutex::new(watcher::WatcherInfo::default()));

    if let Some(ref root) = watcher_root {
        let watcher_index = Arc::clone(&index);
        let watcher_root_clone = root.clone();
        let watcher_info_clone = Arc::clone(&watcher_info);
        tokio::spawn(async move {
            watcher::run_watcher(watcher_root_clone, watcher_index, watcher_info_clone).await;
        });
        tracing::info!("file watcher started");
    }

    // Kick off background git temporal analysis (non-blocking).
    if let Some(ref root) = watcher_root {
        let expected_gen = index.current_project_generation();
        live_index::git_temporal::spawn_git_temporal_computation(
            Arc::clone(&index),
            root.clone(),
            expected_gen,
        );
    }

    let periodic_checkpoint = watcher_root.as_ref().and_then(|root| {
        checkpoint_interval_from_env().map(|interval| {
            tracing::info!(
                interval_secs = interval.as_secs(),
                env = persist::CHECKPOINT_INTERVAL_ENV,
                "periodic checkpointing enabled"
            );
            spawn_periodic_checkpoint(Arc::clone(&index), root.clone(), interval)
        })
    });

    // Spawn HTTP sidecar after watcher, before MCP serve.
    // The sidecar shares the same Arc<LiveIndex> so mutations are immediately visible.
    let bind_host =
        std::env::var("SYMFORGE_SIDECAR_BIND").unwrap_or_else(|_| "127.0.0.1".to_string());
    let sidecar_handle =
        sidecar::spawn_sidecar(Arc::clone(&index), &bind_host, watcher_root.clone()).await?;
    tracing::info!(port = sidecar_handle.port, "HTTP sidecar started");

    // Share the sidecar's TokenStats Arc with the MCP server so the health tool
    // can display token savings without an HTTP round-trip. Clone the Arc so
    // `sidecar_handle` remains intact for `shutdown_and_join` below.
    let token_stats = Some(Arc::clone(&sidecar_handle.token_stats));

    // Create MCP server and serve on stdio transport.
    let server = protocol::SymForgeServer::new(
        Arc::clone(&index),
        project_name,
        watcher_info,
        watcher_root.clone(),
        token_stats,
    );
    tracing::info!("starting MCP server on stdio transport");
    let service = serve_server(server, transport::stdio()).await?;

    // Wait for either MCP server shutdown (stdin EOF) or Ctrl+C/SIGTERM.
    tokio::select! {
        result = service.waiting() => { result?; }
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Ctrl+C received, shutting down");
        }
    }

    tracing::info!("MCP server shut down cleanly");

    if let Some(handle) = periodic_checkpoint {
        handle.abort();
    }

    // Serialize index to disk on clean shutdown.
    // Only serialize when auto-index is enabled (i.e., we have a real project root).
    if let Some(ref root) = watcher_root {
        match persist::serialize_shared_index(&index, root) {
            Ok(()) => tracing::info!("index serialized to .symforge/index.bin"),
            Err(e) => tracing::warn!("failed to serialize index on shutdown: {e}"),
        }
    }

    // Shutdown the sidecar now that the MCP server has exited.
    sidecar_handle.shutdown_and_join().await;
    tracing::info!("sidecar shutdown complete");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        StartupIndexLogView, StartupPlan, checkpoint_interval_from_value, local_empty_reason,
        startup_index_log_view, startup_plan,
    };
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime};
    use symforge::live_index::{
        IndexLoadSource, PublishedIndexState, PublishedIndexStatus, SnapshotVerifyState,
    };

    fn published_state(status: PublishedIndexStatus) -> PublishedIndexState {
        PublishedIndexState {
            generation: 7,
            status,
            degraded_summary: None,
            file_count: 12,
            parsed_count: 10,
            partial_parse_count: 1,
            unexpected_partial_parse_count: 1,
            expected_vendor_partial_parse_count: 0,
            expected_generated_partial_parse_count: 0,
            expected_test_fixture_partial_parse_count: 0,
            expected_template_dsl_partial_parse_count: 0,
            expected_framework_partial_parse_count: 0,
            expected_language_partial_parse_count: 0,
            failed_count: 1,
            partial_parse_files: vec!["src/partial.rs".to_string()],
            unexpected_partial_parse_files: vec!["src/partial.rs".to_string()],
            expected_vendor_partial_parse_files: vec![],
            expected_generated_partial_parse_files: vec![],
            expected_test_fixture_partial_parse_files: vec![],
            expected_template_dsl_partial_parse_files: vec![],
            expected_framework_partial_parse_files: vec![],
            expected_language_partial_parse_files: vec![],
            failed_files: vec![("src/failed.rs".to_string(), "syntax error".to_string())],
            symbol_count: 34,
            loaded_at_system: SystemTime::now(),
            load_duration: Duration::from_millis(42),
            load_source: IndexLoadSource::FreshLoad,
            snapshot_verify_state: SnapshotVerifyState::NotNeeded,
            is_empty: false,
            tier_counts: (0, 0, 0),
            local_empty_reason: None,
            untracked_indexed: 0,
            indexed_root: None,
        }
    }

    #[test]
    fn test_startup_index_log_view_uses_published_ready_counts() {
        let published = published_state(PublishedIndexStatus::Ready);

        assert_eq!(
            startup_index_log_view(&published),
            Some(StartupIndexLogView::Ready {
                file_count: 12,
                symbol_count: 34,
                parsed_count: 10,
                partial_parse_count: 1,
                failed_count: 1,
                duration_ms: 42,
            })
        );
    }

    #[test]
    fn test_startup_index_log_view_uses_published_degraded_summary() {
        let mut published = published_state(PublishedIndexStatus::Degraded);
        published.degraded_summary = Some("circuit breaker tripped: 3/10 files failed".to_string());

        assert_eq!(
            startup_index_log_view(&published),
            Some(StartupIndexLogView::Degraded {
                summary: "circuit breaker tripped: 3/10 files failed".to_string(),
            })
        );
    }

    #[test]
    fn test_startup_plan_prefers_daemon_when_root_exists() {
        let root = PathBuf::from("repo");
        assert_eq!(
            startup_plan(true, Some(root.clone()), true),
            StartupPlan::Daemon { root }
        );
    }

    #[test]
    fn test_startup_plan_falls_back_to_local_auto_index_when_daemon_unavailable() {
        let root = PathBuf::from("repo");
        assert_eq!(
            startup_plan(true, Some(root.clone()), false),
            StartupPlan::LocalAutoIndex { root }
        );
    }

    #[test]
    fn test_startup_plan_reports_disabled_auto_index_reason() {
        assert_eq!(
            startup_plan(false, None, false),
            StartupPlan::LocalEmpty {
                reason: local_empty_reason(false).to_string(),
            }
        );
    }

    #[test]
    fn test_startup_plan_reports_missing_root_reason() {
        assert_eq!(
            startup_plan(true, None, false),
            StartupPlan::LocalEmpty {
                reason: local_empty_reason(true).to_string(),
            }
        );
    }

    #[test]
    fn test_checkpoint_interval_is_opt_in_and_bounded() {
        assert_eq!(checkpoint_interval_from_value(None), None);
        assert_eq!(checkpoint_interval_from_value(Some("0")), None);
        assert_eq!(checkpoint_interval_from_value(Some("false")), None);
        assert_eq!(
            checkpoint_interval_from_value(Some("15")),
            Some(Duration::from_secs(30))
        );
        assert_eq!(
            checkpoint_interval_from_value(Some("120")),
            Some(Duration::from_secs(120))
        );
        assert_eq!(
            checkpoint_interval_from_value(Some("999999")),
            Some(Duration::from_secs(3600))
        );
    }
}
