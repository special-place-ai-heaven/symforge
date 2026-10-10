#![cfg(all(feature = "embed", feature = "__test-internals"))]

use std::path::{Path, PathBuf};

/// Prepare the public Git view for `root` and return the handle that owns it.
fn prepared_source(
    root: &Path,
    scratch: &Path,
) -> (
    symforge::embed::ProcessIndexRuntime,
    symforge::embed::EmbeddedSourceHandle,
) {
    let runtime = symforge::embed::ProcessIndexRuntime::acquire().unwrap();
    let source = runtime
        .open_embedded_source_with_options(
            symforge::embed::EmbeddedSourceSpec::current_worktree(root.to_path_buf()),
            symforge::embed::parity::source_options::EmbeddedOpenOptions {
                state: symforge::embed::parity::source_options::EmbeddedStateSelection::MemoryOnly,
                ..Default::default()
            },
        )
        .unwrap();
    let until = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while source.runtime_view().phase != symforge::embed::SourceRuntimePhase::Current {
        assert!(std::time::Instant::now() < until);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let control =
        symforge::embed::parity::host::OperationControl::new(std::time::Duration::from_secs(20))
            .unwrap();
    let prepared = source
        .prepare_git_view(
            &symforge::embed::parity::source_options::GitPreparationOptions::new(
                scratch.to_path_buf(),
                8 * 1024 * 1024,
                1000,
            ),
            &control,
        )
        .expect("public preparation must not read unadmitted global config");
    assert!(!prepared.view_identity().is_empty());
    (runtime, source)
}

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
    let scratch = tempfile::tempdir().unwrap();
    let (_runtime, source) = prepared_source(&root, scratch.path());
    let isolated = source
        .prepared_git_repository_for_test()
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
    assert_eq!(
        ordinary.config().unwrap().get_string("user.name").unwrap(),
        "ambient"
    );
    let scratch = tempfile::tempdir().unwrap();
    let (_runtime, source) = prepared_source(root, scratch.path());
    let isolated = source.prepared_git_repository_for_test().unwrap();
    assert!(
        isolated.config().unwrap().get_string("user.name").is_err(),
        "global config values must not reach the isolated view"
    );
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

/// Initialize `root` with one committed `lib.rs`; the prepared view copies files only.
fn committed_repository(root: &Path) -> git2::Repository {
    let repository = git2::Repository::init(root).unwrap();
    std::fs::write(root.join("lib.rs"), b"pub fn original() {}\n").unwrap();
    let mut index = repository.index().unwrap();
    index.add_path(Path::new("lib.rs")).unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repository.find_tree(tree_id).unwrap();
    let signature = git2::Signature::now("Fixture", "fixture@example.invalid").unwrap();
    repository
        .commit(Some("HEAD"), &signature, &signature, "fixture", &tree, &[])
        .unwrap();
    drop(tree);
    repository
}

#[test]
fn isolated_git_open_keeps_local_attributes_and_excludes_without_ambient_fallbacks() {
    let root = tempfile::tempdir().unwrap();
    let global = tempfile::tempdir().unwrap();
    committed_repository(root.path());
    std::fs::write(root.path().join(".gitattributes"), "local.rs diff\n").unwrap();
    std::fs::write(root.path().join(".git/info/exclude"), "local-ignore.rs\n").unwrap();
    std::fs::create_dir(global.path().join("git")).unwrap();
    std::fs::write(global.path().join("git/attributes"), "lib.rs -diff\n").unwrap();
    std::fs::write(global.path().join("git/ignore"), "skip.rs\n").unwrap();
    std::fs::write(global.path().join(".gitconfig"), "[user]\nname = ambient\n").unwrap();
    let child = symforge::process_util::hidden_command(std::env::current_exe().unwrap())
        .args(["--exact", "child_isolated_git_config", "--nocapture"])
        .env("SYMFORGE_ISOLATED_GIT_CHILD", root.path())
        .env("SYMFORGE_ISOLATED_GIT_ATTRIBUTES", "1")
        .env("HOME", global.path())
        .env("USERPROFILE", global.path())
        .env("XDG_CONFIG_HOME", global.path())
        .env("GIT_CONFIG_GLOBAL", global.path().join(".gitconfig"))
        .env_remove("GIT_CONFIG_COUNT")
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&child.stdout).contains("running 1 test"));
    assert!(
        child.status.success(),
        "isolated attribute/exclude child failed: {}",
        String::from_utf8_lossy(&child.stderr)
    );
}

// Proven libgit2 1.9.4 limit, not a symforge gap: every public repository open
// (git_repository_open_ext/open_bare/open_from_worktree) runs
// obtain_config_and_set_oid_type -> git_repository_config__weakptr ->
// load_config, which parses the global/XDG/system files located through the
// process-global sysdir search path before the caller can call set_config. A
// malformed ambient include therefore refuses the open. The only escapes are a
// patched libgit2 (cannot reach a downstream embedder through [patch]) or
// git_libgit2_opts search-path mutation (process-global, forbidden here).
#[test]
#[ignore = "libgit2 loads global config inside every public open; see comment"]
fn isolated_git_open_does_not_read_global_includes_and_preserves_local_config() {
    let root = tempfile::tempdir().unwrap();
    let global = tempfile::tempdir().unwrap();
    let repository = committed_repository(root.path());
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
    assert!(
        child.status.success(),
        "isolated-config child failed: {}",
        String::from_utf8_lossy(&child.stderr)
    );
}
