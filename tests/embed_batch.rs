#![cfg(feature = "embed")]

use std::fs;
use std::sync::{Arc, atomic::AtomicBool};
use std::time::{Duration, Instant};

use symforge::embed::parity::edit::{
    BatchEditAction, BatchEditRequest, BatchInsertRequest, BatchRenameRequest, DeleteRequest,
    EditApplyAuthority, EditError, EditErrorKind, EditTarget, InsertPosition, InsertRequest,
    ReplaceRequest,
};
use symforge::embed::{
    EmbeddedSourceHandle, EmbeddedSourceSpec, ProcessIndexRuntime, SourceRuntimePhase,
};

fn wait_current(handle: &EmbeddedSourceHandle, after: u64) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let view = handle.runtime_view();
        if view.phase == SourceRuntimePhase::Current && view.source_version > after {
            return;
        }
        assert!(Instant::now() < deadline, "embedded source did not publish");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn mixed_file_batch_commits_once_and_replays_without_reapplying() {
    let repository = tempfile::tempdir().expect("temporary repository");
    git2::Repository::init(repository.path()).expect("initialize repository");
    fs::create_dir_all(repository.path().join("src")).expect("create source directory");
    let first_path = repository.path().join("src/first.rs");
    let second_path = repository.path().join("src/second.rs");
    fs::write(&first_path, b"pub fn first() -> u32 { 1 }\n").expect("first source");
    fs::write(&second_path, b"pub fn second() -> u32 { 2 }\n").expect("second source");

    let runtime = ProcessIndexRuntime::acquire().expect("acquire runtime");
    let handle = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(
            repository.path().to_path_buf(),
        ))
        .expect("open source");
    wait_current(&handle, 0);
    let first = handle
        .edit_plan(&EditTarget {
            path: "src/first.rs".to_string(),
            name: "first".to_string(),
            kind: None,
            symbol_line: None,
        })
        .expect("first plan");
    let second = handle
        .edit_plan(&EditTarget {
            path: "src/second.rs".to_string(),
            name: "second".to_string(),
            kind: None,
            symbol_line: None,
        })
        .expect("second plan");
    let request = BatchEditRequest {
        actions: vec![
            BatchEditAction::Replace(ReplaceRequest {
                guard: first.guard.clone(),
                new_body: "pub fn first() -> u32 { 10 }".to_string(),
            }),
            BatchEditAction::Replace(ReplaceRequest {
                guard: second.guard.clone(),
                new_body: "pub fn second() -> u32 { 20 }".to_string(),
            }),
        ],
    };
    let preview = handle.preview_batch_edit(&request).expect("batch preview");
    assert_eq!(preview.files.len(), 2);
    assert!(preview.rendered.contains("+ pub fn first() -> u32 { 10 }"));
    assert!(preview.rendered.contains("+ pub fn second() -> u32 { 20 }"));
    assert!(!preview.redacted);
    assert_eq!(
        fs::read(&first_path).expect("first before apply"),
        b"pub fn first() -> u32 { 1 }\n"
    );
    assert_eq!(
        fs::read(&second_path).expect("second before apply"),
        b"pub fn second() -> u32 { 2 }\n"
    );

    let authority = EditApplyAuthority::for_source_root(
        fs::canonicalize(repository.path()).expect("canonical root"),
        "fixture-room/batch".to_string(),
        Arc::new(AtomicBool::new(false)),
    )
    .expect("host authority");
    let applied = handle
        .apply_batch_edit(&request, &authority, "fixture-batch-op")
        .expect("batch apply");
    assert!(!applied.replayed);
    assert_eq!(applied.files.len(), 2);
    assert!(
        fs::read_to_string(&first_path)
            .expect("first after apply")
            .contains("{ 10 }")
    );
    assert!(
        fs::read_to_string(&second_path)
            .expect("second after apply")
            .contains("{ 20 }")
    );

    wait_current(&handle, first.guard.source_version());
    let replayed = handle
        .apply_batch_edit(&request, &authority, "fixture-batch-op")
        .expect("batch replay");
    assert!(replayed.replayed);
    assert_eq!(replayed.files, applied.files);
    let stale = handle
        .apply_batch_edit(&request, &authority, "fixture-batch-op-2")
        .expect_err("stale guard must refuse a new operation");
    assert!(matches!(
        stale,
        EditError::Edit(EditErrorKind::StaleGeneration)
    ));
}

#[test]
fn stale_batch_target_refuses_every_file_before_writing() {
    let repository = tempfile::tempdir().expect("temporary repository");
    git2::Repository::init(repository.path()).expect("initialize repository");
    fs::create_dir_all(repository.path().join("src")).expect("create source directory");
    let first_path = repository.path().join("src/first.rs");
    let second_path = repository.path().join("src/second.rs");
    fs::write(&first_path, b"pub fn first() -> u32 { 1 }\n").expect("first source");
    fs::write(&second_path, b"pub fn second() -> u32 { 2 }\n").expect("second source");
    let runtime = ProcessIndexRuntime::acquire().expect("acquire runtime");
    let handle = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(
            repository.path().to_path_buf(),
        ))
        .expect("open source");
    wait_current(&handle, 0);
    let first = handle
        .edit_plan(&EditTarget {
            path: "src/first.rs".to_string(),
            name: "first".to_string(),
            kind: None,
            symbol_line: None,
        })
        .expect("first plan");
    let second = handle
        .edit_plan(&EditTarget {
            path: "src/second.rs".to_string(),
            name: "second".to_string(),
            kind: None,
            symbol_line: None,
        })
        .expect("second plan");
    let request = BatchEditRequest {
        actions: vec![
            BatchEditAction::Replace(ReplaceRequest {
                guard: first.guard,
                new_body: "pub fn first() -> u32 { 10 }".to_string(),
            }),
            BatchEditAction::Replace(ReplaceRequest {
                guard: second.guard,
                new_body: "pub fn second() -> u32 { 20 }".to_string(),
            }),
        ],
    };
    fs::write(&second_path, b"pub fn second() -> u32 { 3 }\n").expect("external edit");
    let authority = EditApplyAuthority::for_source_root(
        fs::canonicalize(repository.path()).expect("canonical root"),
        "fixture-room/batch-stale".to_string(),
        Arc::new(AtomicBool::new(false)),
    )
    .expect("host authority");
    assert!(
        handle
            .apply_batch_edit(&request, &authority, "fixture-batch-stale-op")
            .is_err()
    );
    assert_eq!(
        fs::read(&first_path).expect("first after refusal"),
        b"pub fn first() -> u32 { 1 }\n"
    );
    assert_eq!(
        fs::read(&second_path).expect("second after refusal"),
        b"pub fn second() -> u32 { 3 }\n"
    );
}

#[test]
fn batch_insert_uses_one_transaction_for_multiple_anchors() {
    let repository = tempfile::tempdir().expect("temporary repository");
    git2::Repository::init(repository.path()).expect("initialize repository");
    fs::create_dir_all(repository.path().join("src")).expect("create source directory");
    let paths = [
        repository.path().join("src/first.rs"),
        repository.path().join("src/second.rs"),
    ];
    fs::write(&paths[0], b"pub fn first() {}\n").expect("first source");
    fs::write(&paths[1], b"pub fn second() {}\n").expect("second source");
    let runtime = ProcessIndexRuntime::acquire().expect("acquire runtime");
    let handle = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(
            repository.path().to_path_buf(),
        ))
        .expect("open source");
    wait_current(&handle, 0);
    let guards = [("src/first.rs", "first"), ("src/second.rs", "second")]
        .into_iter()
        .map(|(path, name)| {
            handle
                .edit_plan(&EditTarget {
                    path: path.to_string(),
                    name: name.to_string(),
                    kind: None,
                    symbol_line: None,
                })
                .expect("plan anchor")
                .guard
        })
        .collect();
    let request = BatchInsertRequest {
        targets: guards,
        position: InsertPosition::Before,
        content: "pub fn helper() {}".to_string(),
    };
    assert_eq!(
        handle
            .preview_batch_insert(&request)
            .expect("preview")
            .files
            .len(),
        2
    );
    let authority = EditApplyAuthority::for_source_root(
        fs::canonicalize(repository.path()).expect("canonical root"),
        "fixture-room/batch-insert".to_string(),
        Arc::new(AtomicBool::new(false)),
    )
    .expect("host authority");
    let applied = handle
        .apply_batch_insert(&request, &authority, "fixture-batch-insert-op")
        .expect("apply insert");
    assert_eq!(applied.files.len(), 2);
    assert!(paths.iter().all(|path| {
        fs::read_to_string(path)
            .expect("inserted source")
            .contains("fn helper()")
    }));
}

#[test]
fn batch_rename_uses_shared_references_and_replays_after_refresh() {
    let repository = tempfile::tempdir().expect("temporary repository");
    git2::Repository::init(repository.path()).expect("initialize repository");
    fs::create_dir_all(repository.path().join("src")).expect("create source directory");
    let definition = repository.path().join("src/definition.rs");
    let usage = repository.path().join("src/usage.rs");
    fs::write(&definition, b"pub struct OldName;\n").expect("definition source");
    fs::write(
        &usage,
        b"pub fn use_it(value: OldName) { let _ = value; }\n",
    )
    .expect("usage source");
    let runtime = ProcessIndexRuntime::acquire().expect("acquire runtime");
    let handle = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(
            repository.path().to_path_buf(),
        ))
        .expect("open source");
    wait_current(&handle, 0);
    let plan = handle
        .plan_batch_rename(
            &EditTarget {
                path: "src/definition.rs".to_string(),
                name: "OldName".to_string(),
                kind: None,
                symbol_line: None,
            },
            true,
        )
        .expect("plan rename");
    assert_eq!(plan.affected_paths.len(), 2);
    let before = plan.guard.source_version();
    let request = BatchRenameRequest {
        plan,
        new_name: "NewName".to_string(),
    };
    let preview = handle
        .preview_batch_rename(&request)
        .expect("rename preview");
    assert_eq!(preview.changes.files.len(), 2);
    assert!(preview.rendered.contains("Confident matches"));
    assert!(!preview.truncated);
    let authority = EditApplyAuthority::for_source_root(
        fs::canonicalize(repository.path()).expect("canonical root"),
        "fixture-room/rename".to_string(),
        Arc::new(AtomicBool::new(false)),
    )
    .expect("host authority");
    let applied = handle
        .apply_batch_rename(&request, &authority, "fixture-rename-op")
        .expect("apply rename");
    assert!(!applied.replayed);
    assert!(
        fs::read_to_string(&definition)
            .expect("definition after")
            .contains("NewName")
    );
    assert!(
        fs::read_to_string(&usage)
            .expect("usage after")
            .contains("NewName")
    );
    wait_current(&handle, before);
    assert!(
        handle
            .apply_batch_rename(&request, &authority, "fixture-rename-op")
            .expect("replay rename")
            .replayed
    );
}

#[test]
fn same_line_delete_and_insert_refuse_overlapping_splices() {
    let repository = tempfile::tempdir().expect("temporary repository");
    git2::Repository::init(repository.path()).expect("initialize repository");
    fs::create_dir_all(repository.path().join("src")).expect("create source directory");
    let path = repository.path().join("src/lib.rs");
    let original = b"pub fn first() {} pub fn second() {}\n";
    fs::write(&path, original).expect("source");
    let runtime = ProcessIndexRuntime::acquire().expect("acquire runtime");
    let handle = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(
            repository.path().to_path_buf(),
        ))
        .expect("open source");
    wait_current(&handle, 0);
    let guard_for = |name: &str| {
        handle
            .edit_plan(&EditTarget {
                path: "src/lib.rs".to_string(),
                name: name.to_string(),
                kind: None,
                symbol_line: None,
            })
            .expect("plan symbol")
            .guard
    };
    let request = BatchEditRequest {
        actions: vec![
            BatchEditAction::Delete(DeleteRequest {
                guard: guard_for("first"),
            }),
            BatchEditAction::Insert(InsertRequest {
                guard: guard_for("second"),
                position: InsertPosition::Before,
                content: "pub fn helper() {}".to_string(),
            }),
        ],
    };
    assert!(matches!(
        handle.preview_batch_edit(&request),
        Err(EditError::Edit(EditErrorKind::WriteConflict))
    ));
    let authority = EditApplyAuthority::for_source_root(
        fs::canonicalize(repository.path()).expect("canonical root"),
        "fixture-room/batch-overlap".to_string(),
        Arc::new(AtomicBool::new(false)),
    )
    .expect("host authority");
    assert!(matches!(
        handle.apply_batch_edit(&request, &authority, "fixture-batch-overlap-op"),
        Err(EditError::Edit(EditErrorKind::WriteConflict))
    ));
    assert_eq!(fs::read(&path).expect("source after refusal"), original);
}

#[test]
fn same_line_replace_and_insert_refuse_actual_replacement_splice_overlap() {
    let repository = tempfile::tempdir().expect("temporary repository");
    git2::Repository::init(repository.path()).expect("initialize repository");
    fs::create_dir_all(repository.path().join("src")).expect("create source directory");
    let path = repository.path().join("src/lib.rs");
    let original = b"pub fn first() {} pub fn second() {}\n";
    fs::write(&path, original).expect("source");
    let runtime = ProcessIndexRuntime::acquire().expect("acquire runtime");
    let handle = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(
            repository.path().to_path_buf(),
        ))
        .expect("open source");
    wait_current(&handle, 0);
    let guard_for = |name: &str| {
        handle
            .edit_plan(&EditTarget {
                path: "src/lib.rs".to_string(),
                name: name.to_string(),
                kind: None,
                symbol_line: None,
            })
            .expect("plan symbol")
            .guard
    };
    let request = BatchEditRequest {
        actions: vec![
            BatchEditAction::Insert(InsertRequest {
                guard: guard_for("first"),
                position: InsertPosition::Before,
                content: "pub fn helper() {}".to_string(),
            }),
            BatchEditAction::Replace(ReplaceRequest {
                guard: guard_for("second"),
                new_body: "pub fn second() { changed() }".to_string(),
            }),
        ],
    };
    assert!(matches!(
        handle.preview_batch_edit(&request),
        Err(EditError::Edit(EditErrorKind::WriteConflict))
    ));
    assert_eq!(fs::read(&path).expect("source after refusal"), original);
}

#[test]
fn delete_cleanup_refuses_batch_when_it_shifts_another_anchor() {
    let repository = tempfile::tempdir().expect("temporary repository");
    git2::Repository::init(repository.path()).expect("initialize repository");
    fs::create_dir_all(repository.path().join("src")).expect("create source directory");
    let path = repository.path().join("src/lib.rs");
    let original = format!(
        "pub fn first() {{}}\n{}pub fn second() {{}}\npub fn third() {{}}\npub fn fourth() {{ let marker = 1234567890; }}\n",
        "\n".repeat(31)
    );
    fs::write(&path, original.as_bytes()).expect("source");
    let runtime = ProcessIndexRuntime::acquire().expect("acquire runtime");
    let handle = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(
            repository.path().to_path_buf(),
        ))
        .expect("open source");
    wait_current(&handle, 0);
    let guard_for = |name: &str| {
        handle
            .edit_plan(&EditTarget {
                path: "src/lib.rs".to_string(),
                name: name.to_string(),
                kind: None,
                symbol_line: None,
            })
            .expect("plan symbol")
            .guard
    };
    let request = BatchEditRequest {
        actions: vec![
            BatchEditAction::Insert(InsertRequest {
                guard: guard_for("second"),
                position: InsertPosition::Before,
                content: "pub fn helper() {}".to_string(),
            }),
            BatchEditAction::Delete(DeleteRequest {
                guard: guard_for("third"),
            }),
        ],
    };
    assert!(matches!(
        handle.preview_batch_edit(&request),
        Err(EditError::Edit(EditErrorKind::WriteConflict))
    ));
    assert_eq!(
        fs::read(&path).expect("source after refusal"),
        original.as_bytes()
    );
}
