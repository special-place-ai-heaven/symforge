#![cfg(all(feature = "embed", feature = "__test-internals"))]

use std::path::{Path, PathBuf};

// The opt-in dependency flag is deliberately exercised through the safe git2
// API. No raw Repository handle is constructed or borrowed by this fixture.
const LOCAL_CONFIG_ONLY: u32 = 1 << 5;

#[test]
fn child_isolated_git_config() {
    let Some(root) = std::env::var_os("SYMFORGE_ISOLATED_GIT_CHILD") else {
        return;
    };
    let root = PathBuf::from(root);
    if std::env::var_os("SYMFORGE_ISOLATED_GIT_ATTRIBUTES").is_some() {
        check_attribute_and_exclude_isolation(&root);
        return;
    }
    let regular = git2::Repository::open_ext(
        root.join(".git"),
        git2::RepositoryOpenFlags::NO_SEARCH | git2::RepositoryOpenFlags::NO_DOTGIT,
        &[] as &[&std::ffi::OsStr],
    );
    assert!(
        regular.is_err(),
        "ordinary open must observe the malformed global fixture"
    );
    let runtime = symforge::embed::ProcessIndexRuntime::acquire().unwrap();
    let source = runtime
        .open_embedded_source_with_options(
            symforge::embed::EmbeddedSourceSpec::current_worktree(root.clone()),
            symforge::embed::parity::source_options::EmbeddedOpenOptions {
                state: symforge::embed::parity::source_options::EmbeddedStateSelection::MemoryOnly,
            },
        )
        .unwrap();
    let until = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while source.runtime_view().phase != symforge::embed::SourceRuntimePhase::Current {
        assert!(std::time::Instant::now() < until);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let scratch = tempfile::tempdir().unwrap();
    let control =
        symforge::embed::parity::host::OperationControl::new(std::time::Duration::from_secs(20))
            .unwrap();
    let prepared = source
        .prepare_git_view(
            &symforge::embed::parity::source_options::GitPreparationOptions::new(
                scratch.path().to_path_buf(),
                8 * 1024 * 1024,
                1000,
            ),
            &control,
        )
        .expect("public preparation must not read unadmitted global config");
    assert!(!prepared.view_identity().is_empty());
    let isolated = git2::Repository::open_ext(
        root.join(".git"),
        git2::RepositoryOpenFlags::NO_SEARCH
            | git2::RepositoryOpenFlags::NO_DOTGIT
            | git2::RepositoryOpenFlags::from_bits_retain(LOCAL_CONFIG_ONLY),
        &[] as &[&std::ffi::OsStr],
    )
    .expect("isolated open must not parse unadmitted global configuration");
    assert_eq!(
        isolated
            .config()
            .unwrap()
            .get_bool("core.filemode")
            .unwrap(),
        false
    );
    let commit = isolated.head().unwrap().peel_to_commit().unwrap();
    let entry = commit
        .tree()
        .unwrap()
        .get_path(Path::new("lib.rs"))
        .unwrap();
    assert_eq!(
        isolated.find_blob(entry.id()).unwrap().content(),
        b"pub fn original() {}\n"
    );
}

fn check_attribute_and_exclude_isolation(root: &Path) {
    let ordinary = git2::Repository::open(root).unwrap();
    let attribute = ordinary
        .get_attr(
            Path::new("lib.rs"),
            "diff",
            git2::AttrCheckFlags::FILE_THEN_INDEX,
        )
        .unwrap();
    assert_eq!(
        git2::AttrValue::from_string(attribute),
        git2::AttrValue::False
    );
    assert!(ordinary.status_should_ignore(Path::new("skip.rs")).unwrap());
    let isolated = git2::Repository::open_ext(
        root.join(".git"),
        git2::RepositoryOpenFlags::NO_SEARCH
            | git2::RepositoryOpenFlags::NO_DOTGIT
            | git2::RepositoryOpenFlags::from_bits_retain(LOCAL_CONFIG_ONLY),
        &[] as &[&std::ffi::OsStr],
    )
    .unwrap();
    let attribute = isolated
        .get_attr(
            Path::new("lib.rs"),
            "diff",
            git2::AttrCheckFlags::FILE_THEN_INDEX,
        )
        .unwrap();
    assert_eq!(
        git2::AttrValue::from_string(attribute),
        git2::AttrValue::Unspecified
    );
    assert!(!isolated.status_should_ignore(Path::new("skip.rs")).unwrap());
    let local = isolated
        .get_attr(
            Path::new("local.rs"),
            "diff",
            git2::AttrCheckFlags::FILE_THEN_INDEX,
        )
        .unwrap();
    assert_eq!(git2::AttrValue::from_string(local), git2::AttrValue::True);
    assert!(
        isolated
            .status_should_ignore(Path::new("local-ignore.rs"))
            .unwrap()
    );
}

#[test]
fn isolated_git_open_keeps_local_attributes_and_excludes_without_ambient_fallbacks() {
    let root = tempfile::tempdir().unwrap();
    let global = tempfile::tempdir().unwrap();
    git2::Repository::init(root.path()).unwrap();
    std::fs::write(root.path().join(".gitattributes"), "local.rs diff\n").unwrap();
    std::fs::write(root.path().join(".git/info/exclude"), "local-ignore.rs\n").unwrap();
    std::fs::create_dir(global.path().join("git")).unwrap();
    std::fs::write(global.path().join("git/attributes"), "lib.rs -diff\n").unwrap();
    std::fs::write(global.path().join("git/ignore"), "skip.rs\n").unwrap();
    let child = symforge::process_util::hidden_command(std::env::current_exe().unwrap())
        .args(["--exact", "child_isolated_git_config", "--nocapture"])
        .env("SYMFORGE_ISOLATED_GIT_CHILD", root.path())
        .env("SYMFORGE_ISOLATED_GIT_ATTRIBUTES", "1")
        .env("HOME", global.path())
        .env("USERPROFILE", global.path())
        .env("XDG_CONFIG_HOME", global.path())
        .env_remove("GIT_CONFIG_COUNT")
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&child.stdout).contains("running 1 test"));
    assert!(
        child.status.success(),
        "isolated attribute/exclude child failed"
    );
}

#[test]
fn isolated_git_open_does_not_read_global_includes_and_preserves_local_config() {
    let root = tempfile::tempdir().unwrap();
    let global = tempfile::tempdir().unwrap();
    let repository = git2::Repository::init(root.path()).unwrap();
    std::fs::write(root.path().join("lib.rs"), b"pub fn original() {}\n").unwrap();
    let mut index = repository.index().unwrap();
    index.add_path(Path::new("lib.rs")).unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repository.find_tree(tree_id).unwrap();
    let signature = git2::Signature::now("Fixture", "fixture@example.invalid").unwrap();
    repository
        .commit(Some("HEAD"), &signature, &signature, "fixture", &tree, &[])
        .unwrap();
    repository
        .config()
        .unwrap()
        .set_bool("core.filemode", false)
        .unwrap();
    std::fs::write(
        global.path().join(".gitconfig"),
        "[include]\npath = broken-config\n",
    )
    .unwrap();
    std::fs::write(global.path().join("broken-config"), "[unfinished\n").unwrap();
    let child = symforge::process_util::hidden_command(std::env::current_exe().unwrap())
        .args(["--exact", "child_isolated_git_config", "--nocapture"])
        .env("SYMFORGE_ISOLATED_GIT_CHILD", root.path())
        .env("HOME", global.path())
        .env("USERPROFILE", global.path())
        .env("XDG_CONFIG_HOME", global.path())
        .env("GIT_CONFIG_GLOBAL", global.path().join(".gitconfig"))
        .env_remove("GIT_CONFIG_COUNT")
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&child.stdout).contains("running 1 test"));
    assert!(child.status.success(), "isolated-config child failed");
}
