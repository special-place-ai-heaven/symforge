#![cfg(feature = "embed")]

use std::fs;
use std::sync::{Arc, atomic::AtomicBool};
use std::time::{Duration, Instant};

use symforge::embed::parity::edit::{
    DeleteRequest, EditApplyAuthority, EditError, EditErrorKind, EditTarget, EditWithinRequest,
    InsertPosition, InsertRequest, ReplaceRequest,
};

use symforge::embed::parity::edit::{WireEditReply, WireEditRequest};

#[test]
fn serialized_edit_plan_preview_and_host_authorized_apply() {
    let repository = tempfile::tempdir().expect("temporary repository");
    git2::Repository::init(repository.path()).expect("initialize repository");
    fs::create_dir_all(repository.path().join("src")).expect("source directory");
    let path = repository.path().join("src/lib.rs");
    fs::write(&path, b"pub fn number() -> u32 { 1 }\n").expect("source");
    let runtime = ProcessIndexRuntime::acquire().expect("runtime");
    let handle = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(
            repository.path().to_path_buf(),
        ))
        .expect("bound source");
    wait_current(&handle, 0);
    let plan_request: WireEditRequest = serde_json::from_slice(
        &serde_json::to_vec(&WireEditRequest::Plan {
            target: EditTarget {
                path: "src/lib.rs".to_string(),
                name: "number".to_string(),
                kind: None,
                symbol_line: None,
            },
        })
        .expect("serialize plan"),
    )
    .expect("deserialize plan");
    let WireEditReply::Plan(plan) = handle.preview_wire_edit(&plan_request).expect("wire plan")
    else {
        panic!("expected plan reply");
    };
    let mut forged = serde_json::to_value(&WireEditRequest::PreviewDelete {
        guard: plan.guard.clone(),
    })
    .unwrap();
    forged["authority"] = serde_json::json!({"grant": true});
    assert!(serde_json::from_value::<WireEditRequest>(forged).is_err());
    let preview_request = WireEditRequest::PreviewReplace {
        guard: plan.guard.clone(),
        new_body: "pub fn number() -> u32 { 2 }".to_string(),
    };
    let WireEditReply::ReplacePreview(preview) = handle
        .preview_wire_edit(&preview_request)
        .expect("wire preview")
    else {
        panic!("expected preview reply");
    };
    assert!(preview.rendered.contains("+ pub fn number() -> u32 { 2 }"));
    assert_eq!(fs::read(&path).unwrap(), b"pub fn number() -> u32 { 1 }\n");
    let apply_request: WireEditRequest = serde_json::from_slice(
        &serde_json::to_vec(&WireEditRequest::ApplyReplace {
            guard: plan.guard,
            new_body: "pub fn number() -> u32 { 2 }".to_string(),
        })
        .unwrap(),
    )
    .expect("deserialize guarded apply");
    assert!(apply_request.is_apply());
    assert!(handle.preview_wire_edit(&apply_request).is_err());
    let authority = EditApplyAuthority::for_source_root(
        fs::canonicalize(repository.path()).unwrap(),
        "fixture-room/wire-edit".to_string(),
        Arc::new(AtomicBool::new(false)),
    )
    .expect("host grant");
    let WireEditReply::ReplaceApplied(applied) = handle
        .apply_wire_edit(&apply_request, &authority, "fixture-wire-edit-op")
        .expect("wire apply")
    else {
        panic!("expected applied reply");
    };
    assert!(!applied.replayed);
    assert!(fs::read_to_string(&path).unwrap().contains("{ 2 }"));
}
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
fn guarded_replace_previews_applies_replays_and_refuses_stale_generation() {
    let repository = tempfile::tempdir().expect("temporary repository");
    git2::Repository::init(repository.path()).expect("initialize repository");
    fs::create_dir_all(repository.path().join("src")).expect("create source directory");
    let path = repository.path().join("src/lib.rs");
    let original = b"pub fn target() -> u32 { 1 }\n";
    let replacement = b"pub fn target() -> u32 { 2 }\n";
    // The symbol span excludes the source's trailing newline. MCP's splice
    // retains that byte after the replacement's own trailing newline.
    let expected_written = b"pub fn target() -> u32 { 2 }\n\n";
    fs::write(&path, original).expect("write source");

    let runtime = ProcessIndexRuntime::acquire().expect("acquire runtime");
    let handle = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(
            repository.path().to_path_buf(),
        ))
        .expect("open source");
    wait_current(&handle, 0);
    let plan = handle
        .edit_plan(&EditTarget {
            path: "src/lib.rs".to_string(),
            name: "target".to_string(),
            kind: None,
            symbol_line: None,
        })
        .expect("plan exact target");
    let request = ReplaceRequest {
        guard: plan.guard.clone(),
        new_body: String::from_utf8(replacement.to_vec()).expect("UTF-8 replacement"),
    };
    let preview = handle
        .preview_replace(&request)
        .expect("preview replacement");
    assert_ne!(preview.old_file_hash, preview.proposed_file_hash);
    assert!(preview.rendered.contains("- pub fn target() -> u32 { 1 }"));
    assert!(preview.rendered.contains("+ pub fn target() -> u32 { 2 }"));
    assert!(!preview.redacted);
    assert!(!preview.truncated);
    assert!(!format!("{preview:?}").contains("pub fn target"));
    assert_eq!(fs::read(&path).expect("read after preview"), original);

    let authority = EditApplyAuthority::for_source_root(
        fs::canonicalize(repository.path()).expect("canonical root"),
        "fixture-room/source".to_string(),
        Arc::new(AtomicBool::new(false)),
    )
    .expect("host authority");
    let applied = handle
        .apply_replace(&request, &authority, "fixture-operation-1")
        .expect("guarded apply");
    assert!(!applied.replayed);
    assert_eq!(applied.post_image_hash, preview.proposed_file_hash);
    assert!(applied.refresh_ticket_identity.is_some());
    assert_eq!(
        fs::read(&path).expect("read applied source"),
        expected_written
    );

    wait_current(&handle, plan.guard.source_version());
    let replayed = handle
        .apply_replace(&request, &authority, "fixture-operation-1")
        .expect("replay completed apply");
    assert!(replayed.replayed);
    let stale = handle
        .apply_replace(&request, &authority, "fixture-operation-2")
        .expect_err("stale publication must refuse");
    assert!(matches!(
        stale,
        EditError::Edit(EditErrorKind::StaleGeneration)
    ));
    assert_eq!(
        fs::read(&path).expect("read after refusal"),
        expected_written
    );
}

#[test]
fn insert_then_delete_use_the_same_guarded_source_lane() {
    let repository = tempfile::tempdir().expect("temporary repository");
    git2::Repository::init(repository.path()).expect("initialize repository");
    fs::create_dir_all(repository.path().join("src")).expect("create source directory");
    let path = repository.path().join("src/lib.rs");
    fs::write(&path, b"pub fn target() -> u32 { 1 }\n").expect("write source");
    let runtime = ProcessIndexRuntime::acquire().expect("acquire runtime");
    let handle = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(
            repository.path().to_path_buf(),
        ))
        .expect("open source");
    wait_current(&handle, 0);
    let authority = EditApplyAuthority::for_source_root(
        fs::canonicalize(repository.path()).expect("canonical root"),
        "fixture-room/structural".to_string(),
        Arc::new(AtomicBool::new(false)),
    )
    .expect("host authority");
    let target = EditTarget {
        path: "src/lib.rs".to_string(),
        name: "target".to_string(),
        kind: None,
        symbol_line: None,
    };
    let plan = handle.edit_plan(&target).expect("plan target");
    let insert = InsertRequest {
        guard: plan.guard.clone(),
        position: InsertPosition::Before,
        content: "pub fn inserted() -> u32 { 0 }".to_string(),
    };
    let insert_preview = handle.preview_insert(&insert).expect("preview insert");
    let inserted = handle
        .apply_insert(&insert, &authority, "fixture-insert-op")
        .expect("apply insert");
    assert_eq!(inserted.post_image_hash, insert_preview.proposed_file_hash);
    assert!(
        fs::read_to_string(&path)
            .expect("inserted file")
            .contains("fn inserted()")
    );
    wait_current(&handle, plan.guard.source_version());

    let inserted_plan = handle
        .edit_plan(&EditTarget {
            path: "src/lib.rs".to_string(),
            name: "inserted".to_string(),
            kind: None,
            symbol_line: None,
        })
        .expect("plan inserted symbol");
    let delete = DeleteRequest {
        guard: inserted_plan.guard.clone(),
    };
    let delete_preview = handle.preview_delete(&delete).expect("preview delete");
    let deleted = handle
        .apply_delete(&delete, &authority, "fixture-delete-op")
        .expect("apply delete");
    assert_eq!(deleted.post_image_hash, delete_preview.proposed_file_hash);
    assert!(
        !fs::read_to_string(&path)
            .expect("deleted file")
            .contains("fn inserted()")
    );
}

#[test]
fn edit_within_targets_exact_occurrence_and_replays() {
    let repository = tempfile::tempdir().expect("temporary repository");
    git2::Repository::init(repository.path()).expect("initialize repository");
    fs::create_dir_all(repository.path().join("src")).expect("create source directory");
    let path = repository.path().join("src/lib.rs");
    fs::write(
        &path,
        b"pub fn target() -> u32 {\n    let first = 1;\n    let second = 1;\n    first + second\n}\n",
    ).expect("write source");
    let runtime = ProcessIndexRuntime::acquire().expect("acquire runtime");
    let handle = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(
            repository.path().to_path_buf(),
        ))
        .expect("open source");
    wait_current(&handle, 0);
    let plan = handle
        .edit_plan(&EditTarget {
            path: "src/lib.rs".to_string(),
            name: "target".to_string(),
            kind: None,
            symbol_line: None,
        })
        .expect("plan target");
    let request = EditWithinRequest {
        guard: plan.guard.clone(),
        old_text: "= 1".to_string(),
        new_text: "= 2".to_string(),
        replace_all: false,
        occurrence: Some(2),
        near_line: None,
    };
    let preview = handle
        .preview_edit_within(&request)
        .expect("preview targeted edit");
    assert_eq!(preview.replacement_count, 1);
    let authority = EditApplyAuthority::for_source_root(
        fs::canonicalize(repository.path()).expect("canonical root"),
        "fixture-room/within".to_string(),
        Arc::new(AtomicBool::new(false)),
    )
    .expect("host authority");
    let applied = handle
        .apply_edit_within(&request, &authority, "fixture-within-op")
        .expect("apply targeted edit");
    assert_eq!(applied.post_image_hash, preview.change.proposed_file_hash);
    let output = fs::read_to_string(&path).expect("edited source");
    assert!(output.contains("let first = 1;"));
    assert!(output.contains("let second = 2;"));
    wait_current(&handle, plan.guard.source_version());
    assert!(
        handle
            .apply_edit_within(&request, &authority, "fixture-within-op")
            .expect("replay")
            .replayed
    );
}

#[test]
fn cancelled_before_write_leaves_source_unchanged() {
    let repository = tempfile::tempdir().expect("temporary repository");
    git2::Repository::init(repository.path()).expect("initialize repository");
    fs::create_dir_all(repository.path().join("src")).expect("create source directory");
    let path = repository.path().join("src/lib.rs");
    let original = b"pub fn target() -> u32 { 1 }\n";
    fs::write(&path, original).expect("write source");
    let runtime = ProcessIndexRuntime::acquire().expect("acquire runtime");
    let handle = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(
            repository.path().to_path_buf(),
        ))
        .expect("open source");
    wait_current(&handle, 0);
    let plan = handle
        .edit_plan(&EditTarget {
            path: "src/lib.rs".to_string(),
            name: "target".to_string(),
            kind: None,
            symbol_line: None,
        })
        .expect("plan exact target");
    let request = ReplaceRequest {
        guard: plan.guard,
        new_body: "pub fn target() -> u32 { 2 }\n".to_string(),
    };
    let authority = EditApplyAuthority::for_source_root(
        fs::canonicalize(repository.path()).expect("canonical root"),
        "fixture-room/cancelled-source".to_string(),
        Arc::new(AtomicBool::new(true)),
    )
    .expect("host authority");
    let refused = handle
        .apply_replace(&request, &authority, "fixture-operation-cancel")
        .expect_err("cancelled apply must refuse");
    assert!(matches!(refused, EditError::Edit(EditErrorKind::Cancelled)));
    assert_eq!(fs::read(&path).expect("read after cancellation"), original);
}
