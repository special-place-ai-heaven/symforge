//! Embed side of the per-tool edit answer golden shared with
//! `tests/edit_body_parity.rs` (`tests/fixtures/edit_parity/tools.json`):
//! every embedded edit lane renders the answer MCP's matching edit tool
//! returns for the same edit, byte for byte apart from the timestamped tee
//! snapshot path, which both sides compare as `<tee>`. Each apply's tee hint
//! names a snapshot of the original written under the source's state.
#![cfg(feature = "embed")]

use std::fs;
use std::path::Path;
use std::sync::{Arc, atomic::AtomicBool};
use std::time::{Duration, Instant};

use symforge::embed::parity::edit::{
    BatchEditAction, BatchEditRequest, BatchInsertRequest, BatchRenameRequest, DeleteRequest,
    EditApplyAuthority, EditGuard, EditTarget, EditWithinRequest, InsertPosition, InsertRequest,
    ReplaceRequest,
};
use symforge::embed::{
    EmbeddedSourceHandle, EmbeddedSourceSpec, ProcessIndexRuntime, SourceRuntimePhase,
};

fn normalize_tee(text: &str) -> String {
    text.lines()
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

/// The relative tee path an answer names and the file it preserves.
fn tee_path(text: &str) -> Option<(String, String)> {
    text.lines().find_map(|line| {
        let (tee, rest) = line
            .trim_start()
            .strip_prefix("Tee snapshot: `")?
            .split_once('`')?;
        let (original, _) = rest.strip_prefix(" preserves `")?.split_once('`')?;
        Some((tee.to_string(), original.to_string()))
    })
}

fn fixture_repo(files: &serde_json::Map<String, serde_json::Value>) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    git2::Repository::init(dir.path()).expect("git init");
    fs::write(dir.path().join(".gitignore"), ".symforge/\n").expect("gitignore");
    for (path, content) in files {
        let file = dir.path().join(path);
        fs::create_dir_all(file.parent().unwrap()).expect("parent");
        fs::write(file, content.as_str().unwrap()).expect("file");
    }
    dir
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

fn guard(handle: &EmbeddedSourceHandle, path: &str, name: &str) -> EditGuard {
    handle
        .edit_plan(&EditTarget {
            path: path.to_string(),
            name: name.to_string(),
            kind: None,
            symbol_line: None,
        })
        .expect("edit plan")
        .guard
}

fn text(input: &serde_json::Value, key: &str) -> String {
    input[key].as_str().unwrap_or_default().to_string()
}

fn position(input: &serde_json::Value) -> InsertPosition {
    if input["position"].as_str() == Some("before") {
        InsertPosition::Before
    } else {
        InsertPosition::After
    }
}

/// Run one MCP tool input on the embedded lanes and render its answer.
fn run_case(
    handle: &EmbeddedSourceHandle,
    authority: &EditApplyAuthority,
    tool: &str,
    input: &serde_json::Value,
    key: &str,
) -> String {
    let dry_run = input["dry_run"].as_bool() == Some(true);
    let path = text(input, "path");
    let name = text(input, "name");
    match tool {
        "replace_symbol_body" => {
            let request = ReplaceRequest {
                guard: guard(handle, &path, &name),
                new_body: text(input, "new_body"),
            };
            if dry_run {
                handle.preview_replace(&request).unwrap().body.render()
            } else {
                handle
                    .apply_replace(&request, authority, key)
                    .unwrap()
                    .body
                    .render()
            }
        }
        "insert_symbol" => {
            let request = InsertRequest {
                guard: guard(handle, &path, &name),
                position: position(input),
                content: text(input, "content"),
            };
            if dry_run {
                handle.preview_insert(&request).unwrap().body.render()
            } else {
                handle
                    .apply_insert(&request, authority, key)
                    .unwrap()
                    .body
                    .render()
            }
        }
        "delete_symbol" => {
            let request = DeleteRequest {
                guard: guard(handle, &path, &name),
            };
            if dry_run {
                handle.preview_delete(&request).unwrap().body.render()
            } else {
                handle
                    .apply_delete(&request, authority, key)
                    .unwrap()
                    .body
                    .render()
            }
        }
        "edit_within_symbol" => {
            let request = EditWithinRequest {
                guard: guard(handle, &path, &name),
                old_text: text(input, "old_text"),
                new_text: text(input, "new_text"),
                replace_all: false,
                occurrence: None,
                near_line: None,
            };
            if dry_run {
                handle
                    .preview_edit_within(&request)
                    .unwrap()
                    .change
                    .body
                    .render()
            } else {
                handle
                    .apply_edit_within(&request, authority, key)
                    .unwrap()
                    .body
                    .render()
            }
        }
        "batch_edit" => {
            let actions = input["edits"]
                .as_array()
                .unwrap()
                .iter()
                .map(|edit| {
                    let guard = guard(handle, &text(edit, "path"), &text(edit, "name"));
                    let operation = &edit["operation"];
                    match operation["type"].as_str().unwrap() {
                        "replace" => BatchEditAction::Replace(ReplaceRequest {
                            guard,
                            new_body: text(operation, "new_body"),
                        }),
                        "delete" => BatchEditAction::Delete(DeleteRequest { guard }),
                        other => panic!("unsupported fixture operation {other}"),
                    }
                })
                .collect();
            let request = BatchEditRequest { actions };
            if dry_run {
                handle.preview_batch_edit(&request).unwrap().body.render()
            } else {
                handle
                    .apply_batch_edit(&request, authority, key)
                    .unwrap()
                    .body
                    .render()
            }
        }
        "batch_insert" => {
            let request = BatchInsertRequest {
                targets: input["targets"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|target| guard(handle, &text(target, "path"), &text(target, "name")))
                    .collect(),
                position: position(input),
                content: text(input, "content"),
            };
            if dry_run {
                handle.preview_batch_insert(&request).unwrap().body.render()
            } else {
                handle
                    .apply_batch_insert(&request, authority, key)
                    .unwrap()
                    .body
                    .render()
            }
        }
        "batch_rename" => {
            let plan = handle
                .plan_batch_rename(
                    &EditTarget {
                        path,
                        name,
                        kind: None,
                        symbol_line: None,
                    },
                    false,
                )
                .unwrap();
            let request = BatchRenameRequest {
                plan,
                new_name: text(input, "new_name"),
            };
            if dry_run {
                handle
                    .preview_batch_rename(&request)
                    .unwrap()
                    .changes
                    .body
                    .render()
            } else {
                handle
                    .apply_batch_rename(&request, authority, key)
                    .unwrap()
                    .body
                    .render()
            }
        }
        other => panic!("unsupported fixture tool {other}"),
    }
}

#[test]
fn embedded_edit_lanes_render_the_mcp_answer_for_every_edit_tool() {
    let fixture: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/edit_parity/tools.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let files = fixture["files"].as_object().unwrap();
    let runtime = ProcessIndexRuntime::acquire().expect("runtime");
    for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let name = case["name"].as_str().unwrap();
        let repo = fixture_repo(files);
        let handle = open(&runtime, repo.path());
        let authority = EditApplyAuthority::for_source_root(
            fs::canonicalize(repo.path()).unwrap(),
            "fixture-room/edit-bodies".to_string(),
            Arc::new(AtomicBool::new(false)),
        )
        .expect("host grant");
        let rendered = run_case(
            &handle,
            &authority,
            case["tool"].as_str().unwrap(),
            &case["input"],
            &format!("edit-bodies-{index}"),
        );
        assert_eq!(
            normalize_tee(&rendered),
            case["expected"].as_str().expect("golden answer"),
            "{name}: embedded answer differs from MCP's"
        );
        if let Some((tee, original)) = tee_path(&rendered) {
            assert_eq!(
                fs::read_to_string(repo.path().join(&tee))
                    .expect("tee snapshot in the source state"),
                files[&original].as_str().unwrap(),
                "{name}: the tee snapshot preserves the original"
            );
        }
        handle.close().expect("close source");
    }
}

/// A project carrying `.symforge/config.toml` gets MCP's trust suffix. With
/// no host control directory the store is unavailable, reported in MCP's
/// exact words for a process without a user-local control directory; with
/// one, the host's store at `embed-host/edit-safety/trust.json` is read.
#[test]
fn project_config_trust_suffix_reads_the_host_store() {
    use symforge::embed::parity::source_options::EmbeddedOpenOptions;
    let mut files = serde_json::Map::new();
    files.insert(
        "src/lib.rs".into(),
        serde_json::Value::String("pub fn number() -> u32 {\n    1\n}\n".into()),
    );
    let runtime = ProcessIndexRuntime::acquire().expect("runtime");
    let preview = |handle: &EmbeddedSourceHandle| {
        handle
            .preview_replace(&ReplaceRequest {
                guard: guard(handle, "src/lib.rs", "number"),
                new_body: "pub fn number() -> u32 {\n    2\n}".into(),
            })
            .unwrap()
            .body
            .render()
    };

    let repo = fixture_repo(&files);
    fs::create_dir_all(repo.path().join(".symforge")).unwrap();
    fs::write(repo.path().join(".symforge/config.toml"), "[index]\n").unwrap();
    let handle = open(&runtime, repo.path());
    let unavailable = preview(&handle);
    assert!(
        unavailable.ends_with(
            "\nProjectConfigTrustWarning: status=Unavailable warning=\"could not determine user-local data directory\"; mode=LOG_ONLY; operation_allowed=true"
        ),
        "{unavailable}"
    );
    handle.close().unwrap();

    let control = tempfile::tempdir().unwrap();
    let handle = runtime
        .open_embedded_source_with_options(
            EmbeddedSourceSpec::current_worktree(repo.path().to_path_buf()),
            EmbeddedOpenOptions {
                replay_control_directory: Some(fs::canonicalize(control.path()).unwrap()),
                ..Default::default()
            },
        )
        .expect("bound source");
    let until = Instant::now() + Duration::from_secs(20);
    while handle.runtime_view().phase != SourceRuntimePhase::Current {
        assert!(Instant::now() < until, "source failed to publish");
        std::thread::sleep(Duration::from_millis(5));
    }
    let untrusted = preview(&handle);
    let store = control.path().join("embed-host/edit-safety/trust.json");
    assert!(
        untrusted.contains("\nProjectConfigTrustWarning: status=Untrusted actual_hash="),
        "{untrusted}"
    );
    assert!(
        untrusted.contains(&format!(
            "trust store {} does not exist",
            fs::canonicalize(control.path())
                .unwrap()
                .join("embed-host/edit-safety/trust.json")
                .display()
        )) || untrusted.contains(&format!("trust store {} does not exist", store.display())),
        "{untrusted}"
    );
    assert!(
        untrusted.ends_with("; mode=LOG_ONLY; operation_allowed=true"),
        "{untrusted}"
    );
    handle.close().unwrap();
}

/// The host chooses the trust mode: `LogOnly` (default) applies an edit over
/// an untrusted project config and carries MCP's warning suffix; `Enforce`
/// refuses preview and apply with MCP's `ProjectConfigTrustEnforced` text
/// before any write.
#[test]
fn project_config_trust_mode_is_the_hosts_choice() {
    use symforge::embed::parity::edit::{EditError, EditErrorKind};
    use symforge::embed::parity::source_options::{EmbeddedOpenOptions, ProjectConfigTrustMode};
    let original = "pub fn number() -> u32 {\n    1\n}\n";
    let mut files = serde_json::Map::new();
    files.insert("src/lib.rs".into(), original.into());
    let runtime = ProcessIndexRuntime::acquire().expect("runtime");
    let repo = fixture_repo(&files);
    fs::create_dir_all(repo.path().join(".symforge")).unwrap();
    fs::write(repo.path().join(".symforge/config.toml"), "[index]\n").unwrap();
    let control = tempfile::tempdir().unwrap();
    let open_with = |mode| {
        let handle = runtime
            .open_embedded_source_with_options(
                EmbeddedSourceSpec::current_worktree(repo.path().to_path_buf()),
                EmbeddedOpenOptions {
                    replay_control_directory: Some(fs::canonicalize(control.path()).unwrap()),
                    project_config_trust_mode: mode,
                    ..Default::default()
                },
            )
            .expect("bound source");
        let until = Instant::now() + Duration::from_secs(20);
        while handle.runtime_view().phase != SourceRuntimePhase::Current {
            assert!(Instant::now() < until, "source failed to publish");
            std::thread::sleep(Duration::from_millis(5));
        }
        handle
    };
    let request = |handle: &EmbeddedSourceHandle| ReplaceRequest {
        guard: guard(handle, "src/lib.rs", "number"),
        new_body: "pub fn number() -> u32 {\n    2\n}".into(),
    };

    let handle = open_with(ProjectConfigTrustMode::Enforce);
    let request_enforced = request(&handle);
    let authority = EditApplyAuthority::for_source_root(
        fs::canonicalize(repo.path()).unwrap(),
        "fixture-room/source".to_string(),
        Arc::new(AtomicBool::new(false)),
    )
    .unwrap();
    let refusals = [
        handle
            .preview_replace(&request_enforced)
            .map(|_| ())
            .unwrap_err(),
        handle
            .apply_replace(&request_enforced, &authority, "trust-enforce-1")
            .map(|_| ())
            .unwrap_err(),
    ];
    for refusal in refusals {
        let EditError::Edit(EditErrorKind::ProjectConfigTrustEnforced { message }) = refusal else {
            panic!("expected the enforced trust refusal, got {refusal:?}");
        };
        assert!(
            message.starts_with("ProjectConfigTrustEnforced: status=Untrusted actual_hash="),
            "{message}"
        );
        assert!(
            message.contains(
                "; mode=ENFORCE; operation_allowed=false; run `symforge trust project-config accept --project "
            ) && message.ends_with("` with reviewed actual_hash before retrying"),
            "{message}"
        );
    }
    assert_eq!(
        fs::read_to_string(repo.path().join("src/lib.rs")).unwrap(),
        original,
        "an enforced refusal writes nothing"
    );
    handle.close().unwrap();

    let handle = open_with(ProjectConfigTrustMode::LogOnly);
    let applied = handle
        .apply_replace(&request(&handle), &authority, "trust-logonly-1")
        .expect("log-only applies");
    assert!(
        applied
            .body
            .render()
            .contains("\nProjectConfigTrustWarning: status=Untrusted actual_hash="),
        "{}",
        applied.body.render()
    );
    assert_ne!(
        fs::read_to_string(repo.path().join("src/lib.rs")).unwrap(),
        original
    );
    handle.close().unwrap();
}
