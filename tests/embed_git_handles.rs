#![cfg(all(feature = "embed", unix))]

use std::os::fd::AsRawFd;

/// Regression evidence for the rejected descriptor-path Repository shortcut.
/// libgit2 reopens the original spelling, so a descriptor name is not confinement.
#[test]
fn libgit2_repository_descriptor_path_does_not_confine_object_reads() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("source");
    let moved = parent.path().join("original");
    let original = git2::Repository::init(&root).unwrap();
    std::fs::write(root.join("lib.rs"), b"pub fn original() {}\n").unwrap();
    let mut index = original.index().unwrap();
    index.add_path(std::path::Path::new("lib.rs")).unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = original.find_tree(tree_id).unwrap();
    let signature = git2::Signature::now("fixture", "fixture@example.invalid").unwrap();
    let oid = original
        .commit(Some("HEAD"), &signature, &signature, "original", &tree, &[])
        .unwrap();
    drop(tree);
    drop(index);
    drop(original);

    let directory =
        cap_std::fs::Dir::open_ambient_dir(root.join(".git"), cap_std::ambient_authority())
            .unwrap();
    let descriptor_path = format!("/proc/self/fd/{}", directory.as_raw_fd());
    let retained = git2::Repository::open_bare(&descriptor_path).unwrap();
    std::fs::rename(&root, &moved).unwrap();
    let replacement = git2::Repository::init(&root).unwrap();
    std::fs::write(root.join("lib.rs"), b"pub fn replacement() {}\n").unwrap();
    assert!(replacement.find_commit(oid).is_err());

    assert!(retained.find_commit(oid).is_err());
    let foreign_blob = replacement.blob(b"replacement object").unwrap();
    assert_eq!(
        retained.find_blob(foreign_blob).unwrap().content(),
        b"replacement object"
    );
}

#[test]
fn libgit2_odb_alternate_keeps_the_opened_objects_directory_after_root_rename() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("source");
    let moved = parent.path().join("original");
    let original = git2::Repository::init(&root).unwrap();
    let blob_id = original.blob(b"original object bytes").unwrap();
    drop(original);
    let objects =
        cap_std::fs::Dir::open_ambient_dir(root.join(".git/objects"), cap_std::ambient_authority())
            .unwrap();
    let descriptor_path = format!("/proc/self/fd/{}", objects.as_raw_fd());
    let odb = git2::Odb::new().unwrap();
    odb.add_disk_alternate(&descriptor_path).unwrap();
    let retained = git2::Repository::from_odb(odb).unwrap();
    std::fs::rename(&root, &moved).unwrap();
    let replacement = git2::Repository::init(&root).unwrap();
    replacement.blob(b"replacement object bytes").unwrap();
    assert!(replacement.find_blob(blob_id).is_err());
    assert_eq!(
        retained.find_blob(blob_id).unwrap().content(),
        b"original object bytes"
    );
}
