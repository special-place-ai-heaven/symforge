//! Embed side of the query golden shared with the MCP lib test
//! `change_queries_match_embed_parity_golden` (`src/protocol/tools.rs`,
//! `tests/fixtures/query_parity/changes.json`): `what_changed`,
//! `diff_symbols`, `detect_impact`, `explore`, `ask` and the
//! `symforge://repo/changes/uncommitted` resource render MCP's bytes for the
//! same repository. Commit ids and the root are compared as placeholders.
#![cfg(feature = "embed")]

use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use symforge::embed::parity::ask::AskRequest;
use symforge::embed::parity::changes::{DiffSymbolsRequest, WhatChangedRequest};
use symforge::embed::parity::detect_impact::DetectImpactRequest;
use symforge::embed::parity::guidance::ExploreRequest;
use symforge::embed::parity::host::{
    HostLimits, HostRequest, HostResourceContent, HostResourceRequest, HostResponse, HostRights,
    HostRoom, HostRoomConfig, HostRoomGrant, OperationControl,
};
use symforge::embed::parity::source_options::GitPreparationOptions;
use symforge::embed::parity::{QueryLimits, QueryOutput, QueryRequest};
use symforge::embed::{
    EmbeddedSourceHandle, EmbeddedSourceSpec, ProcessIndexRuntime, SourceRuntimePhase,
};

/// The fixture repository: commits with a fixed signature and time, then
/// uncommitted working files. Returns the commit ids in order.
fn query_parity_repo(spec: &serde_json::Value, root: &Path) -> Vec<String> {
    let repository = git2::Repository::init(root).expect("init");
    let signature = git2::Signature::new(
        "Fixture",
        "fixture@example.invalid",
        &git2::Time::new(1_700_000_000, 0),
    )
    .expect("signature");
    let mut commits: Vec<String> = Vec::new();
    for commit in spec["commits"].as_array().expect("commits") {
        let mut index = repository.index().expect("index");
        for (path, content) in commit["files"].as_object().expect("files") {
            let file = root.join(path);
            fs::create_dir_all(file.parent().expect("parent")).expect("dir");
            fs::write(&file, content.as_str().expect("content")).expect("write");
            index.add_path(Path::new(path)).expect("add");
        }
        index.write().expect("index write");
        let tree = repository
            .find_tree(index.write_tree().expect("tree"))
            .expect("find tree");
        let parent = commits.last().map(|id| {
            repository
                .find_commit(git2::Oid::from_str(id).unwrap())
                .unwrap()
        });
        let parents: Vec<&git2::Commit<'_>> = parent.iter().collect();
        let id = repository
            .commit(
                Some("HEAD"),
                &signature,
                &signature,
                "fixture",
                &tree,
                &parents,
            )
            .expect("commit");
        commits.push(id.to_string());
    }
    for (path, content) in spec["working"].as_object().expect("working") {
        let file = root.join(path);
        fs::create_dir_all(file.parent().expect("parent")).expect("dir");
        fs::write(&file, content.as_str().expect("content")).expect("write");
    }
    commits
}

fn query_parity_input(input: &serde_json::Value, commits: &[String]) -> serde_json::Value {
    let text = serde_json::to_string(input)
        .unwrap()
        .replace("{base}", &commits[0])
        .replace("{target}", &commits[1]);
    serde_json::from_str(&text).unwrap()
}

fn query_parity_text(text: &str, commits: &[String], root: &Path) -> String {
    let mut folded = text.to_string();
    for (id, placeholder) in commits.iter().zip(["{base}", "{target}"]) {
        folded = folded.replace(id.as_str(), placeholder);
        for short in [12, 8, 7] {
            folded = folded.replace(&id[..short], placeholder);
        }
    }
    let mut roots = vec![root.display().to_string()];
    if let Ok(canonical) = dunce::canonicalize(root) {
        roots.push(canonical.display().to_string());
    }
    let slashed: Vec<String> = roots.iter().map(|root| root.replace('\\', "/")).collect();
    roots.extend(slashed);
    roots.sort_by_key(|root| std::cmp::Reverse(root.len()));
    for root in roots {
        folded = folded.replace(&root, "{root}");
    }
    folded
}

fn open(runtime: &ProcessIndexRuntime, root: &Path) -> EmbeddedSourceHandle {
    let handle = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(root.to_path_buf()))
        .expect("bound source");
    let until = Instant::now() + Duration::from_secs(20);
    while handle.runtime_view().phase != SourceRuntimePhase::Current {
        assert!(Instant::now() < until, "source failed to publish");
        std::thread::sleep(Duration::from_millis(5));
    }
    handle
}

/// The MCP resource read through a host room, as a host serves it.
fn resource_text(root: &Path, uri: &str) -> String {
    assert_eq!(uri, "symforge://repo/changes/uncommitted");
    let grant = HostRoomGrant::new(
        "query-golden-room".to_owned(),
        root.to_path_buf(),
        "query-golden-source".to_owned(),
        HostRights::read_only(),
    )
    .unwrap();
    let room = HostRoom::open(
        ProcessIndexRuntime::acquire().unwrap(),
        grant,
        HostRoomConfig {
            room_id: "query-golden-room".to_owned(),
            limits: HostLimits::default(),
        },
    )
    .unwrap();
    let control = room.control().unwrap();
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        match room.dispatch(
            &HostRequest::Resource(HostResourceRequest::RepoChangesUncommitted),
            &control,
        ) {
            Ok(HostResponse::Resource(reply)) => {
                assert_eq!(reply.uri, uri);
                let HostResourceContent::Query(query) = reply.content else {
                    panic!("the changes resource is a query reply")
                };
                let QueryOutput::WhatChanged(changes) = query.output else {
                    panic!("the changes resource renders what_changed")
                };
                return changes.rendered;
            }
            Ok(other) => panic!("unexpected response {other:?}"),
            Err(_) => {
                assert!(Instant::now() < until, "room never served the resource");
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

fn tool_text(
    handle: &EmbeddedSourceHandle,
    scratch: &Path,
    tool: &str,
    input: &serde_json::Value,
) -> String {
    let session = handle.new_query_session().unwrap();
    let request = match tool {
        "what_changed" => QueryRequest::WhatChanged(
            serde_json::from_value::<WhatChangedRequest>(input.clone()).unwrap(),
        ),
        "diff_symbols" => QueryRequest::DiffSymbols(
            serde_json::from_value::<DiffSymbolsRequest>(input.clone()).unwrap(),
        ),
        "detect_impact" => {
            handle
                .prepare_git_view(
                    &GitPreparationOptions::new(scratch.to_path_buf(), 64 * 1024 * 1024, 5_000),
                    &OperationControl::new(Duration::from_secs(20)).unwrap(),
                )
                .expect("prepared git view");
            QueryRequest::DetectImpact(
                serde_json::from_value::<DetectImpactRequest>(input.clone()).unwrap(),
            )
        }
        "explore" => {
            let mut request = ExploreRequest::new(input["query"].as_str().unwrap());
            if let Some(depth) = input["depth"].as_u64() {
                request.depth = depth as u32;
            }
            QueryRequest::Explore(request)
        }
        "ask" => QueryRequest::Ask(AskRequest {
            query: input["query"].as_str().unwrap().to_owned(),
            ..Default::default()
        }),
        other => panic!("unsupported fixture tool {other}"),
    };
    let claim = handle
        .query_with_session(&request, QueryLimits::default(), &session, None)
        .expect("query");
    match claim.value() {
        QueryOutput::WhatChanged(value) => value.rendered.clone(),
        QueryOutput::DiffSymbols(value) => value.rendered.clone(),
        QueryOutput::DetectImpact(value) => value.rendered.clone(),
        QueryOutput::Exploration(value) => value.rendered.clone(),
        QueryOutput::Ask(value) => value.rendered.clone(),
        other => panic!("unexpected output {other:?}"),
    }
}

#[test]
fn change_and_query_lanes_render_the_mcp_answer() {
    let fixture: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/query_parity/changes.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let repo = tempfile::tempdir().unwrap();
        let scratch = tempfile::tempdir().unwrap();
        let commits = query_parity_repo(
            &fixture["repos"][case["repo"].as_str().unwrap()],
            repo.path(),
        );
        let text = match case["resource"].as_str() {
            Some(uri) => resource_text(repo.path(), uri),
            None => {
                let handle = open(&runtime, repo.path());
                let text = tool_text(
                    &handle,
                    scratch.path(),
                    case["tool"].as_str().unwrap(),
                    &query_parity_input(&case["input"], &commits),
                );
                handle.close().unwrap();
                text
            }
        };
        assert_eq!(
            query_parity_text(&text, &commits, repo.path()),
            case["expected"].as_str().expect("golden answer"),
            "{name}: embedded answer differs from MCP's"
        );
    }
}
