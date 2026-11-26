use super::*;
use crate::codemode::{CodeReadCompleteness, CodeReadOp, DaemonCodeQueryRequest};
use axum::response::{IntoResponse, Response};
use serde_json::{Map, json};
use std::sync::atomic::AtomicUsize;
use tempfile::TempDir;

// Literal wire name to catch accidental changes on either side of the seam.
const REVISION_HEADER: &str = "x-symforge-code-query-revision";

fn peer_reply(revision: Option<&str>, body: impl Serialize) -> Response {
    let mut response = Json(body).into_response();
    if let Some(revision) = revision {
        response
            .headers_mut()
            .insert(REVISION_HEADER, revision.parse().unwrap());
    }
    response
}

async fn listen(app: Router) -> (u16, tokio::task::JoinHandle<io::Result<()>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move { axum::serve(listener, app).await });
    (port, server)
}

#[tokio::test]
async fn same_identity_cli_and_stdio_attach_require_the_code_revision() {
    let project = TempDir::new().unwrap();
    let root = project.path().canonicalize().unwrap();
    for (health_revision, open_revision, expected_error, expected_opens, expected_closes) in [
        (None, None, Some("missing (legacy peer)"), 0, 0),
        (Some("99"), Some("99"), Some("got 99"), 0, 0),
        (Some("1"), Some("1"), Some("got 1"), 0, 0),
        (Some("2"), None, Some("missing (legacy peer)"), 2, 2),
        (Some("2"), Some("2"), None, 2, 2),
    ] {
        let home = TempDir::new().unwrap();
        let control = ControlStateDir::new(home.path().to_path_buf());
        let identity = current_daemon_identity();
        let health = DaemonHealth {
            daemon_version: identity.version,
            executable_path: identity.executable_path,
            project_count: 0,
            session_count: 0,
            stale_sessions: 0,
            auth_required: false,
            pid: None,
            started_at_unix_secs: None,
        };
        let receipt = OpenProjectResponse {
            project_id: project_key(&root),
            session_id: "session-test".to_string(),
            project_name: "test".to_string(),
            canonical_root: normalized_path_string(&root),
            session_count: 1,
        };
        let opened = Arc::new(AtomicUsize::new(0));
        let closed = Arc::new(AtomicUsize::new(0));
        let (port, server) = listen(
            Router::new()
                .route(
                    "/health",
                    get(move || {
                        let health = health.clone();
                        async move { peer_reply(health_revision, health) }
                    }),
                )
                .route(
                    "/v1/sessions/open",
                    post({
                        let opened = Arc::clone(&opened);
                        move || {
                            opened.fetch_add(1, Ordering::Relaxed);
                            let receipt = receipt.clone();
                            async move { peer_reply(open_revision, receipt) }
                        }
                    }),
                )
                .route(
                    "/v1/sessions/session-test",
                    delete({
                        let closed = Arc::clone(&closed);
                        let project_id = project_key(&root);
                        move || {
                            closed.fetch_add(1, Ordering::Relaxed);
                            let project_id = project_id.clone();
                            async move {
                                Json(CloseSessionResponse {
                                    session_id: "session-test".to_string(),
                                    project_id,
                                    remaining_sessions: 0,
                                    project_removed: true,
                                })
                            }
                        }
                    }),
                ),
        )
        .await;
        write_daemon_port_file_at(&control, port).unwrap();
        // Deliberately not the health-reported PID: this fixture owns no
        // process that the test could authorize stopping.
        write_daemon_pid_file_at(&control, 999_999).unwrap();
        for result in [
            connect_existing_session_at(&root, "symforge-code-cli", None, &control).await,
            connect_or_spawn_session_at(&root, "mcp-stdio", None, &control)
                .await
                .map(Some),
        ] {
            match (result, expected_error) {
                (Ok(Some(session)), None) => {
                    assert_eq!(session.project_id(), project_key(&root).as_str());
                    session.close().await.unwrap();
                }
                (Err(error), Some(expected)) => {
                    let message = error.to_string();
                    assert!(
                        message.contains("Code Mode daemon protocol mismatch"),
                        "{message}"
                    );
                    assert!(message.contains(expected), "{message}");
                    assert!(message.contains("matching SymForge build"), "{message}");
                }
                (result, _) => panic!(
                    "unexpected attach result: {:?}",
                    result.map(|session| session.is_some())
                ),
            }
        }
        assert_eq!(opened.load(Ordering::Relaxed), expected_opens);
        assert_eq!(closed.load(Ordering::Relaxed), expected_closes);
        assert_eq!(read_daemon_port_file_at(&control).unwrap(), port);
        assert_eq!(read_daemon_pid_file_at(&control).unwrap(), 999_999);
        server.abort();
    }
}

fn query() -> DaemonCodeQueryRequest {
    DaemonCodeQueryRequest {
        op: CodeReadOp::SearchText,
        kwargs: Map::new(),
        project: None,
    }
}

#[tokio::test]
async fn code_query_success_requires_current_revision_before_decoding_completeness() {
    let home = TempDir::new().unwrap();
    for (revision, completeness, expected) in [
        (None, None, None),
        (Some("99"), None, None),
        (Some("1"), None, None),
        (Some("2"), None, Some(0)),
        (
            Some("2"),
            Some(json!({
                "omitted_rows": 3, "withheld_files": 1, "unparsed_files": 2,
                "omitted_rows_inexact": true, "incompatible_grammar_files": 1
            })),
            Some(3),
        ),
    ] {
        let (port, server) = listen(Router::new().route(
            "/v1/sessions/session-test/code-query",
            post(move |headers: HeaderMap| {
                let completeness = completeness.clone();
                async move {
                    assert_eq!(headers.get(REVISION_HEADER).unwrap(), "2");
                    let mut body = json!({"status": "ok", "value": [{"line": 5}]});
                    if let Some(completeness) = completeness {
                        body["completeness"] = completeness;
                    }
                    peer_reply(revision, body)
                }
            }),
        ))
        .await;
        let session = DaemonSessionClient::new_with_auth_token(
            format!("http://127.0.0.1:{port}"),
            "project-test".to_string(),
            "session-test".to_string(),
            "test".to_string(),
            None,
            ControlStateDir::new(home.path().to_path_buf()),
        );
        let outcome = session.call_code_query_with_completeness(&query()).await;
        if let Some(omitted_rows) = expected {
            let (outcome, _) = outcome.unwrap();
            let result = outcome.unwrap();
            assert_eq!(result.value, json!([{"line": 5}]));
            let expected = if omitted_rows == 0 {
                CodeReadCompleteness::default()
            } else {
                CodeReadCompleteness {
                    omitted_rows,
                    withheld_files: 1,
                    unparsed_files: 2,
                    omitted_rows_inexact: true,
                    incompatible_grammar_files: 1,
                }
            };
            assert_eq!(result.completeness, expected);
        } else {
            let message = outcome.unwrap_err().to_string();
            assert!(
                message.contains("Code Mode daemon protocol mismatch"),
                "{message}"
            );
            assert!(
                message.contains(revision.unwrap_or("missing (legacy peer)")),
                "{message}"
            );
        }
        server.abort();
    }
}

#[tokio::test]
async fn current_router_refuses_legacy_and_mismatched_code_query_requests() {
    let token = "test-code-query-revision";
    let home = TempDir::new().unwrap();
    let control = ControlStateDir::new(home.path().to_path_buf());
    let state = Arc::new(DaemonState::with_token_at(token.to_owned(), control));
    let (port, server) = listen(build_router(state)).await;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    let base = format!("http://127.0.0.1:{port}");
    let health = client.get(format!("{base}/health")).send().await.unwrap();
    assert_eq!(health.headers().get(REVISION_HEADER).unwrap(), "2");
    for revision in [None, Some("1"), Some("99"), Some("2")] {
        let request = client
            .post(format!("{base}/v1/sessions/not-a-session/code-query"))
            .bearer_auth(token)
            .json(&query());
        let request = if let Some(revision) = revision {
            request.header(REVISION_HEADER, revision)
        } else {
            request
        };
        let response = request.send().await.unwrap();
        if revision == Some("2") {
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
            assert!(response.text().await.unwrap().contains("unknown session"));
        } else {
            assert_eq!(response.status(), StatusCode::CONFLICT);
            assert_eq!(response.headers().get(REVISION_HEADER).unwrap(), "2");
            let message = response.text().await.unwrap();
            assert!(
                message.contains("Code Mode daemon protocol mismatch"),
                "{message}"
            );
        }
    }
    server.abort();
}
