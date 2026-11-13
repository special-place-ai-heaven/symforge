//! #875 — `symforge serve --project ALIAS=PATH` serves several operator-fixed
//! repositories from one process, routes `project` on HTTP `/mcp`, and refuses a
//! client `index_folder` unless the operator allows it.
//!
//! Every test drives the REAL `serve::run` (bound on `127.0.0.1:0`, loopback only,
//! never a wildcard address) and learns the bound address through `bound_addr_tx`.
//! Every wait on the spawned server is bounded by a timeout.
#![cfg(feature = "server")]

use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use symforge::server::serve::{ServeArgs, ServeError, run};

const WAIT: Duration = Duration::from_secs(120);

/// A fixture repository with one uniquely named file and symbol.
fn fixture_repo(file: &str, symbol: &str) -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().expect("fixture root");
    std::fs::create_dir(dir.path().join(".git")).expect("plant .git");
    std::fs::write(dir.path().join(file), format!("pub fn {symbol}() {{}}\n")).expect("seed");
    dir
}

fn spec(alias: &str, path: &Path) -> String {
    format!("{alias}={}", path.display())
}

fn args(projects: Vec<String>, allow_index_folder: bool) -> ServeArgs {
    ServeArgs {
        listen: "127.0.0.1:0".to_string(),
        explicit_listen: true,
        projects,
        allow_index_folder,
        ..ServeArgs::default()
    }
}

/// A running serve: its `/mcp` URL and the task to abort at the end.
struct Served {
    url: String,
    task: tokio::task::JoinHandle<Result<(), ServeError>>,
}

impl Drop for Served {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn start(mut serve_args: ServeArgs) -> Served {
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    serve_args.bound_addr_tx = Some(tx);
    let task = tokio::spawn(run(serve_args));
    let bound = tokio::task::spawn_blocking(move || rx.recv_timeout(WAIT))
        .await
        .expect("join the address wait");
    let addr: SocketAddr = match bound {
        Ok(Ok(addr)) => addr,
        Ok(Err(bind)) => panic!("serve failed to bind: {bind}"),
        Err(_) => {
            let outcome = tokio::time::timeout(WAIT, task).await;
            panic!("serve never reported a bound address: {outcome:?}");
        }
    };
    assert!(addr.ip().is_loopback(), "tests bind loopback only: {addr}");
    Served {
        url: format!("http://{addr}/mcp"),
        task,
    }
}

/// `run` must refuse before binding; returns the refusal.
async fn refused(serve_args: ServeArgs) -> ServeError {
    tokio::time::timeout(WAIT, run(serve_args))
        .await
        .expect("a startup refusal returns promptly")
        .expect_err("serve must refuse to start")
}

async fn rpc(url: &str, method: &str, params: serde_json::Value) -> serde_json::Value {
    let client = reqwest::Client::builder()
        .timeout(WAIT)
        .build()
        .expect("client");
    let response = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .json(&serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}))
        .send()
        .await
        .expect("request sent");
    assert!(response.status().is_success(), "HTTP {}", response.status());
    response.json().await.expect("JSON-RPC body")
}

/// `tools/call`; returns (text, isError).
async fn call(url: &str, tool: &str, arguments: serde_json::Value) -> (String, bool) {
    let response = rpc(
        url,
        "tools/call",
        serde_json::json!({"name": tool, "arguments": arguments}),
    )
    .await;
    let result = &response["result"];
    let text = result["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("no text content: {response}"))
        .to_string();
    (text, result["isError"] == serde_json::json!(true))
}

struct TwoRepos {
    _a: tempfile::TempDir,
    _b: tempfile::TempDir,
    served: Served,
}

async fn two_repos() -> TwoRepos {
    let a = fixture_repo("alpha.rs", "alpha_only_fn");
    let b = fixture_repo("beta.rs", "beta_only_fn");
    let served = start(args(vec![spec("a", a.path()), spec("b", b.path())], false)).await;
    TwoRepos {
        _a: a,
        _b: b,
        served,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn two_clients_route_by_project_at_the_same_time() {
    let repos = two_repos().await;
    let url = repos.served.url.clone();
    let (a, b) = tokio::join!(
        call(
            &url,
            "search_symbols",
            serde_json::json!({"query": "alpha_only_fn", "project": "a"})
        ),
        call(
            &url,
            "search_symbols",
            serde_json::json!({"query": "beta_only_fn", "project": "b"})
        ),
    );
    assert!(!a.1 && a.0.contains("alpha_only_fn"), "project a: {}", a.0);
    assert!(!b.1 && b.0.contains("beta_only_fn"), "project b: {}", b.0);

    // Each answer comes from its own repository only.
    let (cross, _) = call(
        &url,
        "search_symbols",
        serde_json::json!({"query": "beta_only_fn", "project": "a"}),
    )
    .await;
    assert!(!cross.contains("beta.rs"), "a must not see b: {cross}");
}

#[tokio::test(flavor = "multi_thread")]
async fn missing_or_unknown_project_lists_the_aliases() {
    let repos = two_repos().await;
    let url = &repos.served.url;

    let (missing, is_error) = call(url, "search_symbols", serde_json::json!({"query": "x"})).await;
    assert!(is_error, "missing project is an error: {missing}");
    assert!(
        missing.contains("pass project") && missing.contains("a, b"),
        "{missing}"
    );

    let (unknown, is_error) = call(
        url,
        "search_symbols",
        serde_json::json!({"query": "x", "project": "zzz"}),
    )
    .await;
    assert!(is_error, "unknown project is an error: {unknown}");
    assert!(
        unknown.contains("'zzz'") && unknown.contains("a, b"),
        "{unknown}"
    );

    // A path is never an alias.
    let (path, is_error) = call(
        url,
        "search_symbols",
        serde_json::json!({"query": "x", "project": "C:/somewhere/a"}),
    )
    .await;
    assert!(is_error && path.contains("a, b"), "{path}");
}

#[tokio::test(flavor = "multi_thread")]
async fn one_repository_without_project_behaves_as_today() {
    let a = fixture_repo("alpha.rs", "alpha_only_fn");
    let served = start(ServeArgs {
        workspace_root: Some(a.path().to_path_buf()),
        ..args(Vec::new(), false)
    })
    .await;
    let (text, is_error) = call(
        &served.url,
        "search_symbols",
        serde_json::json!({"query": "alpha_only_fn"}),
    )
    .await;
    assert!(!is_error && text.contains("alpha_only_fn"), "{text}");
}

#[tokio::test(flavor = "multi_thread")]
async fn client_index_folder_is_refused_and_changes_nothing() {
    let repos = two_repos().await;
    let url = &repos.served.url;
    let other = fixture_repo("gamma.rs", "gamma_only_fn");

    for arguments in [
        serde_json::json!({"path": other.path().display().to_string()}),
        serde_json::json!({"path": other.path().display().to_string(), "project": "a"}),
        serde_json::json!({"path": other.path().display().to_string(), "add": true}),
    ] {
        let (text, is_error) = call(url, "index_folder", arguments).await;
        assert!(is_error, "index_folder must be refused: {text}");
        assert!(
            text.contains("fixed by the operator") && text.contains("a, b"),
            "{text}"
        );
    }

    let (a, _) = call(
        url,
        "search_symbols",
        serde_json::json!({"query": "alpha_only_fn", "project": "a"}),
    )
    .await;
    assert!(a.contains("alpha_only_fn"), "a unchanged: {a}");
    let (gamma, _) = call(
        url,
        "search_symbols",
        serde_json::json!({"query": "gamma_only_fn", "project": "a"}),
    )
    .await;
    assert!(!gamma.contains("gamma.rs"), "nothing was indexed: {gamma}");
}

#[tokio::test(flavor = "multi_thread")]
async fn allow_index_folder_permits_it_on_one_repository() {
    let a = fixture_repo("alpha.rs", "alpha_only_fn");
    let other = fixture_repo("gamma.rs", "gamma_only_fn");
    let served = start(args(vec![spec("a", a.path())], true)).await;
    let (text, is_error) = call(
        &served.url,
        "index_folder",
        serde_json::json!({"path": other.path().display().to_string()}),
    )
    .await;
    assert!(!is_error && text.starts_with("Indexed "), "{text}");
    let (gamma, _) = call(
        &served.url,
        "search_symbols",
        serde_json::json!({"query": "gamma_only_fn"}),
    )
    .await;
    assert!(gamma.contains("gamma_only_fn"), "{gamma}");
}

#[tokio::test(flavor = "multi_thread")]
async fn startup_refuses_bad_project_lists() {
    let a = fixture_repo("alpha.rs", "alpha_only_fn");
    let b = fixture_repo("beta.rs", "beta_only_fn");
    let missing = a.path().join("does-not-exist");
    let cases: Vec<(Vec<String>, bool, &str)> = vec![
        (
            vec![spec("a", a.path()), spec("A", b.path())],
            false,
            "duplicate",
        ),
        (vec![spec("a/b", a.path())], false, "alias"),
        (vec![spec("a\\b", a.path())], false, "alias"),
        (vec![spec("*", a.path())], false, "alias"),
        (vec![spec("", a.path())], false, "alias"),
        (vec![spec(" a", a.path())], false, "alias"),
        (vec![a.path().display().to_string()], false, "ALIAS=PATH"),
        (vec![spec("a", &missing)], false, "does not exist"),
        (
            vec![spec("a", a.path()), spec("b", b.path())],
            true,
            "--allow-index-folder",
        ),
    ];
    for (projects, allow, needle) in cases {
        let shown = format!("{projects:?} allow={allow}");
        let err = refused(args(projects, allow)).await;
        assert!(
            matches!(&err, ServeError::Projects(text) if text.contains(needle)),
            "{shown}: expected a --project refusal naming {needle:?}, got {err}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn resource_reads_route_by_project_or_list_the_aliases() {
    let repos = two_repos().await;
    let url = &repos.served.url;

    let routed = rpc(
        url,
        "resources/read",
        serde_json::json!({"uri": "symforge://file/content?path=beta.rs&project=b"}),
    )
    .await;
    let text = routed["result"]["contents"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("routed read: {routed}"));
    assert!(text.contains("beta_only_fn"), "{text}");

    // The same file does not exist in repository a.
    let elsewhere = rpc(
        url,
        "resources/read",
        serde_json::json!({"uri": "symforge://file/content?path=beta.rs&project=a"}),
    )
    .await;
    assert!(
        elsewhere["error"].is_object()
            || !elsewhere["result"]["contents"][0]["text"]
                .as_str()
                .unwrap_or_default()
                .contains("beta_only_fn"),
        "a must not serve b's file: {elsewhere}"
    );

    let ambiguous = rpc(
        url,
        "resources/read",
        serde_json::json!({"uri": "symforge://repo/map"}),
    )
    .await;
    let message = ambiguous["error"]["message"]
        .as_str()
        .unwrap_or_else(|| panic!("ambiguous read must error: {ambiguous}"));
    assert!(message.contains("a, b"), "{message}");

    // Static text needs no repository.
    let glossary = rpc(
        url,
        "resources/read",
        serde_json::json!({"uri": "symforge://glossary"}),
    )
    .await;
    assert!(
        glossary["result"]["contents"][0]["text"].is_string(),
        "{glossary}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn projects_fan_out_answers_from_every_listed_repository() {
    let repos = two_repos().await;
    let url = &repos.served.url;

    let (text, is_error) = call(
        url,
        "search_symbols",
        serde_json::json!({"query": "only_fn", "projects": ["*"]}),
    )
    .await;
    assert!(!is_error, "{text}");
    assert!(
        text.contains("alpha_only_fn") && text.contains("beta_only_fn"),
        "{text}"
    );
    assert!(
        text.contains("project: a") && text.contains("project: b"),
        "{text}"
    );

    let (unknown, is_error) = call(
        url,
        "search_symbols",
        serde_json::json!({"query": "only_fn", "projects": ["a", "zzz"]}),
    )
    .await;
    assert!(
        is_error && unknown.contains("'zzz'") && unknown.contains("a, b"),
        "{unknown}"
    );

    let (unsupported, is_error) = call(
        url,
        "get_repo_map",
        serde_json::json!({"projects": ["a", "b"]}),
    )
    .await;
    assert!(
        is_error && unsupported.contains("pass project"),
        "{unsupported}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn status_and_health_name_the_served_repositories() {
    let repos = two_repos().await;
    let url = &repos.served.url;

    let (inventory, is_error) =
        call(url, "status", serde_json::json!({"detail": "projects"})).await;
    assert!(!is_error, "{inventory}");
    assert!(
        inventory.contains("name=a ") && inventory.contains("name=b "),
        "{inventory}"
    );
    assert!(inventory.contains("watcher="), "{inventory}");

    let (health, is_error) = call(url, "health", serde_json::json!({"project": "b"})).await;
    assert!(!is_error, "{health}");
    assert!(health.contains("Watcher:"), "{health}");
}
