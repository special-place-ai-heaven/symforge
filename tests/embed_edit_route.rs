#![cfg(feature = "embed")]
//! MCP `working_directory` routing for embedded edits. A routed edit lands in
//! a host-admitted linked worktree of the bound repository and reports MCP's
//! resolved target; any other directory refuses before a write and is never
//! opened.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, atomic::AtomicBool};
use std::time::{Duration, Instant};

use symforge::embed::parity::edit::{
    AdmittedEditTarget, BatchEditAction, BatchEditRequest, BatchRenameRequest, EditApplyAuthority,
    EditError, EditErrorKind, EditTarget, ReplaceRequest, RoutedBatchPart,
};
use symforge::embed::{
    EmbeddedSourceHandle, EmbeddedSourceSpec, ProcessIndexRuntime, SourceRuntimePhase,
};

const ORIGINAL: &str = "pub fn number() -> u32 { 1 }\n";

fn commit_all(repository: &git2::Repository, paths: &[&str]) {
    let mut index = repository.index().unwrap();
    for path in paths {
        index.add_path(Path::new(path)).unwrap();
    }
    index.write().unwrap();
    let tree = repository.find_tree(index.write_tree().unwrap()).unwrap();
    let signature = git2::Signature::now("Fixture", "fixture@example.invalid").unwrap();
    repository
        .commit(Some("HEAD"), &signature, &signature, "fixture", &tree, &[])
        .unwrap();
}

/// A committed repository holding `src/lib.rs`.
fn repository(root: &Path) -> git2::Repository {
    let repository = git2::Repository::init(root).unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join(".gitignore"), ".symforge/\n").unwrap();
    fs::write(root.join("src/lib.rs"), ORIGINAL).unwrap();
    commit_all(&repository, &[".gitignore", "src/lib.rs"]);
    repository
}

fn open(runtime: &ProcessIndexRuntime, root: &Path) -> EmbeddedSourceHandle {
    let handle = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(root.to_path_buf()))
        .unwrap();
    let until = Instant::now() + Duration::from_secs(20);
    while handle.runtime_view().phase != SourceRuntimePhase::Current {
        assert!(Instant::now() < until, "source never reached Current");
        std::thread::sleep(Duration::from_millis(5));
    }
    handle
}

/// A linked checkout may materialize CRLF (core.autocrlf); compare by lines.
fn read_lf(path: &Path) -> String {
    fs::read_to_string(path).unwrap().replace("\r\n", "\n")
}

fn authority(root: &Path, scope: &str) -> EditApplyAuthority {
    EditApplyAuthority::for_source_root(
        fs::canonicalize(root).unwrap(),
        scope.to_owned(),
        Arc::new(AtomicBool::new(false)),
    )
    .unwrap()
}

fn plan_number(handle: &EmbeddedSourceHandle) -> symforge::embed::parity::edit::EditPlan {
    handle
        .edit_plan(&EditTarget {
            path: "src/lib.rs".into(),
            name: "number".into(),
            kind: None,
            symbol_line: None,
        })
        .unwrap()
}

struct Fixture {
    _parent: tempfile::TempDir,
    main: PathBuf,
    linked: PathBuf,
}

/// A main worktree and one linked worktree of the same repository.
fn fixture() -> Fixture {
    let parent = tempfile::tempdir().unwrap();
    let main = parent.path().join("main");
    fs::create_dir(&main).unwrap();
    let repository = repository(&main);
    let linked = parent.path().join("linked");
    repository.worktree("linked", &linked, None).unwrap();
    Fixture {
        main: dunce::canonicalize(&main).unwrap(),
        linked: dunce::canonicalize(&linked).unwrap(),
        _parent: parent,
    }
}

#[test]
fn routed_edit_lands_in_the_admitted_linked_worktree_and_reports_like_mcp() {
    let fixture = fixture();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let main = open(&runtime, &fixture.main);
    let linked = open(&runtime, &fixture.linked);
    let linked_authority = authority(&fixture.linked, "room/linked");
    let admitted = [AdmittedEditTarget {
        handle: &linked,
        authority: &linked_authority,
    }];

    let plan = plan_number(&main);
    let route = main
        .route_edit("src/lib.rs", &fixture.linked, &admitted)
        .unwrap();
    assert!(route.resolved.rerouted);
    let wrote_to = fixture.linked.join("src/lib.rs");
    let indexed = fixture.main.join("src/lib.rs");
    assert_eq!(route.resolved.target_path, wrote_to);
    assert_eq!(route.resolved.indexed_path, indexed);
    assert_eq!(
        route.resolved.reroute_suffix(),
        format!(
            "\nworking_directory: {}\nrerouted: true\nwrote_to: {}\nindexed_path: {}",
            fixture.linked.display(),
            wrote_to.display(),
            indexed.display()
        )
    );

    // A batch rename re-plans over the routed worktree's own references.
    let rename = main
        .plan_batch_rename(
            &EditTarget {
                path: "src/lib.rs".into(),
                name: "number".into(),
                kind: None,
                symbol_line: None,
            },
            false,
        )
        .unwrap();
    let routed_rename = main.rebase_rename_plan(&route, &rename).unwrap();
    let routed_handle = route.target.expect("rerouted target").handle;
    let preview = routed_handle
        .preview_batch_rename(&BatchRenameRequest {
            plan: routed_rename,
            new_name: "count".into(),
        })
        .unwrap();
    assert!(preview.rendered.contains("count"), "{}", preview.rendered);

    let guard = main.rebase_guard(&route, &plan.guard).unwrap();
    let target = route.target.expect("rerouted target");
    target
        .handle
        .apply_replace(
            &ReplaceRequest {
                guard,
                new_body: "pub fn number() -> u32 { 2 }".into(),
            },
            target.authority,
            "route-replace-1",
        )
        .unwrap();
    assert_eq!(read_lf(&wrote_to), "pub fn number() -> u32 { 2 }\n");
    assert_eq!(read_lf(&indexed), ORIGINAL);

    // The bound root itself passes through, unrerouted, like MCP.
    let passthrough = main
        .route_edit("src/lib.rs", &fixture.main, &admitted)
        .unwrap();
    assert!(passthrough.target.is_none());
    assert!(!passthrough.resolved.rerouted);
    assert_eq!(passthrough.resolved.target_path, indexed);
    assert_eq!(
        main.rebase_guard(&passthrough, &plan.guard).unwrap(),
        plan.guard
    );
}

#[test]
fn unadmitted_or_unrelated_working_directory_refuses_before_any_write() {
    let fixture = fixture();
    let unrelated_dir = tempfile::tempdir().unwrap();
    repository(unrelated_dir.path());
    let unrelated_root = dunce::canonicalize(unrelated_dir.path()).unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let main = open(&runtime, &fixture.main);
    let unrelated = open(&runtime, &unrelated_root);
    let unrelated_authority = authority(&unrelated_root, "room/unrelated");

    let refusal = |result: Result<_, EditError>| match result {
        Err(EditError::Edit(kind)) => kind,
        Err(other) => panic!("unexpected refusal: {other:?}"),
        Ok(_) => panic!("routing must refuse"),
    };
    // A real linked worktree the host did not admit refuses.
    assert_eq!(
        refusal(main.route_edit("src/lib.rs", &fixture.linked, &[])),
        EditErrorKind::WorkingDirectoryNotAdmitted
    );
    // So does a directory that does not exist: it is never opened.
    assert_eq!(
        refusal(main.route_edit("src/lib.rs", &fixture.main.join("absent"), &[])),
        EditErrorKind::WorkingDirectoryNotAdmitted
    );
    // An admitted source of another repository is not a worktree of this one.
    assert_eq!(
        refusal(main.route_edit(
            "src/lib.rs",
            &unrelated_root,
            &[AdmittedEditTarget {
                handle: &unrelated,
                authority: &unrelated_authority,
            }],
        )),
        EditErrorKind::WorkingDirectoryNotAWorktree
    );
    for root in [&fixture.main, &fixture.linked, &unrelated_root] {
        assert_eq!(read_lf(&root.join("src/lib.rs")), ORIGINAL);
    }
}

const OTHER: &str = "pub fn other() -> u32 { 3 }\n";

/// A main worktree and a linked worktree whose repository tracks two files,
/// so one batch can route `src/other.rs` into the linked worktree while
/// `src/lib.rs` stays in the main one.
fn two_file_fixture() -> Fixture {
    let parent = tempfile::tempdir().unwrap();
    let main = parent.path().join("main");
    fs::create_dir_all(main.join("src")).unwrap();
    let repository = git2::Repository::init(&main).unwrap();
    fs::write(main.join(".gitignore"), ".symforge/\n").unwrap();
    fs::write(main.join("src/lib.rs"), ORIGINAL).unwrap();
    fs::write(main.join("src/other.rs"), OTHER).unwrap();
    commit_all(&repository, &[".gitignore", "src/lib.rs", "src/other.rs"]);
    let linked = parent.path().join("linked");
    repository.worktree("linked", &linked, None).unwrap();
    Fixture {
        main: dunce::canonicalize(&main).unwrap(),
        linked: dunce::canonicalize(&linked).unwrap(),
        _parent: parent,
    }
}

fn plan_of(
    handle: &EmbeddedSourceHandle,
    path: &str,
    name: &str,
) -> symforge::embed::parity::edit::EditPlan {
    handle
        .edit_plan(&EditTarget {
            path: path.into(),
            name: name.into(),
            kind: None,
            symbol_line: None,
        })
        .unwrap()
}

fn wait_current(handles: &[&EmbeddedSourceHandle]) {
    let until = Instant::now() + Duration::from_secs(20);
    for handle in handles {
        while handle.runtime_view().phase != SourceRuntimePhase::Current {
            assert!(Instant::now() < until, "source never returned to Current");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

fn replace(guard: symforge::embed::parity::edit::EditGuard, body: &str) -> BatchEditRequest {
    BatchEditRequest {
        actions: vec![BatchEditAction::Replace(ReplaceRequest {
            guard,
            new_body: body.into(),
        })],
    }
}

/// MCP stages every file of a batch, rerouted or not, into one
/// `commit_staged` run. A batch routed across two admitted worktrees commits
/// both, and its one replay record verifies each worktree's post-image
/// through that worktree's own source authority on retry.
#[test]
fn routed_batch_commits_across_two_worktrees_and_replays_verified_post_images() {
    let fixture = two_file_fixture();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let main = open(&runtime, &fixture.main);
    let linked = open(&runtime, &fixture.linked);
    let main_authority = authority(&fixture.main, "room/main");
    let linked_authority = authority(&fixture.linked, "room/linked");
    let admitted = [AdmittedEditTarget {
        handle: &linked,
        authority: &linked_authority,
    }];

    let lib_plan = plan_of(&main, "src/lib.rs", "number");
    let other_plan = plan_of(&main, "src/other.rs", "other");
    let route = main
        .route_edit("src/other.rs", &fixture.linked, &admitted)
        .unwrap();
    let other_guard = main.rebase_guard(&route, &other_plan.guard).unwrap();
    let parts = [
        RoutedBatchPart {
            handle: &main,
            authority: &main_authority,
            request: replace(lib_plan.guard, "pub fn number() -> u32 { 2 }"),
            working_directory: None,
        },
        RoutedBatchPart {
            handle: &linked,
            authority: &linked_authority,
            request: replace(other_guard, "pub fn other() -> u32 { 4 }"),
            working_directory: Some(fixture.linked.clone()),
        },
    ];

    let applied = main
        .apply_routed_batch_edit(&main_authority, &parts, "routed-batch-1")
        .unwrap();
    assert!(!applied.replayed);
    assert_eq!(applied.parts.len(), 2);
    assert_eq!(applied.parts[0].files[0].0, "src/lib.rs");
    assert_eq!(applied.parts[1].files[0].0, "src/other.rs");
    assert_eq!(
        read_lf(&fixture.main.join("src/lib.rs")),
        "pub fn number() -> u32 { 2 }\n"
    );
    assert_eq!(
        read_lf(&fixture.linked.join("src/other.rs")),
        "pub fn other() -> u32 { 4 }\n"
    );
    assert_eq!(read_lf(&fixture.main.join("src/other.rs")), OTHER);
    assert_eq!(read_lf(&fixture.linked.join("src/lib.rs")), ORIGINAL);

    // An identical retry replays after verifying both post-images.
    wait_current(&[&main, &linked]);
    let replayed = main
        .apply_routed_batch_edit(&main_authority, &parts, "routed-batch-1")
        .unwrap();
    assert!(replayed.replayed);
    for (replay, first) in replayed.parts.iter().zip(&applied.parts) {
        assert_eq!(replay.files, first.files);
    }

    // A changed post-image in the linked worktree breaks the replay proof.
    fs::write(
        fixture.linked.join("src/other.rs"),
        "pub fn other() -> u32 { 5 }\n",
    )
    .unwrap();
    wait_current(&[&main, &linked]);
    match main.apply_routed_batch_edit(&main_authority, &parts, "routed-batch-1") {
        Err(EditError::Edit(EditErrorKind::ReplayConflict)) => {}
        other => panic!("tampered replay must conflict: {other:?}"),
    }
}

/// Every pre-image in every source is verified before any write, as in
/// MCP's single `commit_staged` run: a conflict in one worktree leaves the
/// other untouched.
#[test]
fn routed_batch_conflict_in_one_worktree_writes_nothing_in_either() {
    let fixture = two_file_fixture();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let main = open(&runtime, &fixture.main);
    let linked = open(&runtime, &fixture.linked);
    let main_authority = authority(&fixture.main, "room/main");
    let linked_authority = authority(&fixture.linked, "room/linked");
    let admitted = [AdmittedEditTarget {
        handle: &linked,
        authority: &linked_authority,
    }];
    let lib_plan = plan_of(&main, "src/lib.rs", "number");
    let other_plan = plan_of(&main, "src/other.rs", "other");
    let route = main
        .route_edit("src/other.rs", &fixture.linked, &admitted)
        .unwrap();
    let other_guard = main.rebase_guard(&route, &other_plan.guard).unwrap();

    // Same length and mtime: the worker's scout cannot see it, so only the
    // commit's pre-image check can.
    let lib = fixture.main.join("src/lib.rs");
    let mtime = filetime::FileTime::from_last_modification_time(&fs::metadata(&lib).unwrap());
    let tampered = fs::read_to_string(&lib).unwrap().replace("{ 1 }", "{ 9 }");
    fs::write(&lib, &tampered).unwrap();
    filetime::set_file_mtime(&lib, mtime).unwrap();

    let parts = [
        RoutedBatchPart {
            handle: &main,
            authority: &main_authority,
            request: replace(lib_plan.guard, "pub fn number() -> u32 { 2 }"),
            working_directory: None,
        },
        RoutedBatchPart {
            handle: &linked,
            authority: &linked_authority,
            request: replace(other_guard, "pub fn other() -> u32 { 4 }"),
            working_directory: Some(fixture.linked.clone()),
        },
    ];
    match main.apply_routed_batch_edit(&main_authority, &parts, "routed-batch-conflict") {
        Err(EditError::Edit(EditErrorKind::WriteConflict)) => {}
        other => panic!("a stale pre-image must refuse the whole batch: {other:?}"),
    }
    assert_eq!(fs::read_to_string(&lib).unwrap(), tampered);
    assert_eq!(read_lf(&fixture.linked.join("src/other.rs")), OTHER);
}

// --- Routed edit answers shared with MCP -------------------------------------

/// Substitute `{main}` and `{worktree}` in every string of `value`.
fn with_roots(value: &serde_json::Value, main: &Path, worktree: &Path) -> serde_json::Value {
    use serde_json::Value;
    match value {
        Value::String(text) => Value::String(
            text.replace("{main}", main.to_str().unwrap())
                .replace("{worktree}", worktree.to_str().unwrap()),
        ),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| with_roots(item, main, worktree))
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, item)| (key.clone(), with_roots(item, main, worktree)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Fold both roots back to placeholders and tee paths to `<tee>`.
fn routed_golden_text(text: &str, main: &Path, worktree: &Path) -> String {
    let mut roots = vec![
        (main.display().to_string(), "{main}"),
        (worktree.display().to_string(), "{worktree}"),
    ];
    // Paths outside the root's own display (the tee's preserved original)
    // render with forward slashes.
    let slashed: Vec<(String, &str)> = roots
        .iter()
        .map(|(root, placeholder)| (root.replace('\\', "/"), *placeholder))
        .collect();
    roots.extend(slashed);
    roots.sort_by_key(|(root, _)| std::cmp::Reverse(root.len()));
    let mut folded = text.to_string();
    for (root, placeholder) in roots {
        folded = folded.replace(&root, placeholder);
    }
    // Platform separators: the golden is shared by every host.
    folded
        .replace('\\', "/")
        .lines()
        .map(|line| {
            let trimmed = line.trim_start();
            let indent = &line[..line.len() - trimmed.len()];
            match trimmed
                .strip_prefix("Tee snapshot: `")
                .and_then(|rest| rest.split_once('`'))
            {
                Some((_, tail)) => format!("{indent}Tee snapshot: `<tee>`{tail}"),
                None => line.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// MCP's worktree fixture layout: `<container>/main` and its linked worktree
/// `<container>/wt_one`, with `worktree_files` written into the worktree.
fn routed_golden_fixture(
    files: &serde_json::Map<String, serde_json::Value>,
    worktree_files: &serde_json::Map<String, serde_json::Value>,
) -> Fixture {
    let parent = tempfile::tempdir().unwrap();
    let main = parent.path().join("main");
    let repository = git2::Repository::init(&main).unwrap();
    let mut paths = vec![];
    for (path, content) in files {
        let file = main.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, content.as_str().unwrap()).unwrap();
        paths.push(path.as_str());
    }
    fs::write(main.join(".gitignore"), ".symforge/\n").unwrap();
    paths.push(".gitignore");
    commit_all(&repository, &paths);
    let linked = parent.path().join("wt_one");
    repository.worktree("wt_one", &linked, None).unwrap();
    // A Windows checkout may materialize CRLF; the golden is LF.
    for (path, content) in files {
        fs::write(linked.join(path), content.as_str().unwrap()).unwrap();
    }
    for (path, content) in worktree_files {
        fs::write(linked.join(path), content.as_str().unwrap()).unwrap();
    }
    Fixture {
        main: dunce::canonicalize(&main).unwrap(),
        linked: dunce::canonicalize(&linked).unwrap(),
        _parent: parent,
    }
}

fn routed_text(input: &serde_json::Value, key: &str) -> String {
    input[key].as_str().unwrap_or_default().to_string()
}

/// One MCP batch_edit input as embedded routed parts: actions without a
/// `working_directory` stay on the bound source, the rest route.
fn routed_batch_body(
    main: &EmbeddedSourceHandle,
    main_authority: &EditApplyAuthority,
    admitted: &[AdmittedEditTarget<'_>],
    input: &serde_json::Value,
    key: &str,
) -> String {
    let mut parts: Vec<RoutedBatchPart<'_>> = Vec::new();
    for edit in input["edits"].as_array().unwrap() {
        let edit_path = routed_text(edit, "path");
        let plan = plan_of(main, &edit_path, &routed_text(edit, "name"));
        let new_body = routed_text(&edit["operation"], "new_body");
        let request = |guard| BatchEditRequest {
            actions: vec![BatchEditAction::Replace(ReplaceRequest {
                guard,
                new_body: new_body.clone(),
            })],
        };
        match edit["working_directory"].as_str() {
            Some(working_directory) => {
                let route = main
                    .route_edit(&edit_path, Path::new(working_directory), admitted)
                    .unwrap();
                let guard = main.rebase_guard(&route, &plan.guard).unwrap();
                let target = route.target.expect("rerouted part");
                parts.push(RoutedBatchPart {
                    handle: target.handle,
                    authority: target.authority,
                    request: request(guard),
                    working_directory: Some(PathBuf::from(working_directory)),
                });
            }
            None => parts.push(RoutedBatchPart {
                handle: main,
                authority: main_authority,
                request: request(plan.guard),
                working_directory: None,
            }),
        }
    }
    main.apply_routed_batch_edit(main_authority, &parts, key)
        .unwrap()
        .body
        .render()
}

/// Run one MCP routed input through `route_edit` and the native lanes.
fn run_routed_case(
    main: &EmbeddedSourceHandle,
    main_authority: &EditApplyAuthority,
    admitted: &[AdmittedEditTarget<'_>],
    tool: &str,
    input: &serde_json::Value,
    key: &str,
) -> String {
    use symforge::embed::parity::edit::{EditWithinRequest, InsertPosition, InsertRequest};
    if tool == "batch_edit" {
        return routed_batch_body(main, main_authority, admitted, input, key);
    }
    let path = routed_text(input, "path");
    let name = routed_text(input, "name");
    let dry_run = input["dry_run"].as_bool() == Some(true);
    let working_directory = PathBuf::from(routed_text(input, "working_directory"));
    let route = main
        .route_edit(&path, &working_directory, admitted)
        .unwrap();
    let (handle, authority) = match route.target {
        Some(target) => (target.handle, target.authority),
        None => (main, main_authority),
    };
    let guard = || {
        main.rebase_guard(&route, &plan_of(main, &path, &name).guard)
            .unwrap()
    };
    let body = match tool {
        "replace_symbol_body" => {
            let request = ReplaceRequest {
                guard: guard(),
                new_body: routed_text(input, "new_body"),
            };
            if dry_run {
                handle.preview_replace(&request).unwrap().body
            } else {
                handle.apply_replace(&request, authority, key).unwrap().body
            }
        }
        "edit_within_symbol" => {
            let request = EditWithinRequest {
                guard: guard(),
                old_text: routed_text(input, "old_text"),
                new_text: routed_text(input, "new_text"),
                replace_all: false,
                occurrence: None,
                near_line: None,
            };
            handle
                .apply_edit_within(&request, authority, key)
                .unwrap()
                .body
        }
        "insert_symbol" => {
            let request = InsertRequest {
                guard: guard(),
                position: if input["position"].as_str() == Some("before") {
                    InsertPosition::Before
                } else {
                    InsertPosition::After
                },
                content: routed_text(input, "content"),
            };
            handle.apply_insert(&request, authority, key).unwrap().body
        }
        "batch_rename" => {
            let plan = main
                .plan_batch_rename(
                    &EditTarget {
                        path: path.clone(),
                        name: name.clone(),
                        kind: None,
                        symbol_line: None,
                    },
                    false,
                )
                .unwrap();
            let request = BatchRenameRequest {
                plan: main.rebase_rename_plan(&route, &plan).unwrap(),
                new_name: routed_text(input, "new_name"),
            };
            handle
                .apply_batch_rename(&request, authority, key)
                .unwrap()
                .body
        }
        other => panic!("unsupported routed fixture tool {other}"),
    };
    route.render_body(&body)
}

/// Embed side of the routed edit golden shared with
/// `tests/worktree_awareness.rs` (`tests/fixtures/edit_parity/routed.json`):
/// each routed edit renders MCP's answer, reroute report included.
#[test]
fn routed_edit_lanes_render_the_mcp_answer() {
    let golden: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/edit_parity/routed.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let files = golden["files"].as_object().unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    for (index, case) in golden["cases"].as_array().unwrap().iter().enumerate() {
        let name = case["name"].as_str().unwrap();
        let fixture = routed_golden_fixture(files, case["worktree_files"].as_object().unwrap());
        let main = open(&runtime, &fixture.main);
        let linked = open(&runtime, &fixture.linked);
        let main_authority = authority(&fixture.main, "room/main");
        let linked_authority = authority(&fixture.linked, "room/linked");
        let admitted = [AdmittedEditTarget {
            handle: &linked,
            authority: &linked_authority,
        }];
        let input = with_roots(&case["input"], &fixture.main, &fixture.linked);
        let rendered = run_routed_case(
            &main,
            &main_authority,
            &admitted,
            case["tool"].as_str().unwrap(),
            &input,
            &format!("routed-golden-{index}"),
        );
        assert_eq!(
            routed_golden_text(&rendered, &fixture.main, &fixture.linked),
            case["expected"].as_str().expect("golden answer"),
            "{name}: embedded routed answer differs from MCP's"
        );
        main.close().unwrap();
        linked.close().unwrap();
    }
}
