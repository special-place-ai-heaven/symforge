#![cfg(all(feature = "embed", feature = "__test-internals"))]

use std::fs;
use std::time::{Duration, Instant};

use symforge::embed::parity::host::{
    HostIndexFailureCause, HostLimits, HostPhase, HostRequest, HostResponse, HostRights, HostRoom,
    HostRoomConfig, HostRoomGrant, HostRuntimeOwner,
};
use symforge::embed::parity::{QueryLimits, QueryOutput, QueryRequest};
use symforge::embed::{
    EmbeddedSourceHandle, EmbeddedSourceSpec, ProcessIndexRuntime, SourceRuntimePhase,
};

fn settle(handle: &EmbeddedSourceHandle, after: u64) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let view = handle.runtime_view();
        if view.phase == SourceRuntimePhase::Current && view.source_version > after {
            return;
        }
        assert!(Instant::now() < deadline, "source did not settle: {view:?}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn git(root: &std::path::Path, args: &[&str]) {
    let status = symforge::process_util::hidden_command("git")
        .args(args)
        .current_dir(root)
        .env("GIT_AUTHOR_NAME", "symforge")
        .env("GIT_AUTHOR_EMAIL", "symforge@example.invalid")
        .env("GIT_COMMITTER_NAME", "symforge")
        .env("GIT_COMMITTER_EMAIL", "symforge@example.invalid")
        .status()
        .expect("run git");
    assert!(status.success(), "git {args:?} failed");
}

#[test]
fn s4_query_symbol_exposes_doc_byte_range_for_rust_ts_python() {
    let root = tempfile::tempdir().unwrap();
    git2::Repository::init(root.path()).unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::write(
        root.path().join("src/lib.rs"),
        "/// rust docs\npub fn rust_docced(x: u32) -> u32 { x }\n",
    )
    .unwrap();
    fs::write(
        root.path().join("src/mod.ts"),
        "/** ts docs */\nexport function tsDocced(x: number): number { return x; }\n",
    )
    .unwrap();
    fs::write(
        root.path().join("src/mod.py"),
        "\"\"\"py docs\"\"\"\ndef py_docced(x):\n    return x\n",
    )
    .unwrap();

    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(
            root.path().to_path_buf(),
        ))
        .unwrap();
    settle(&handle, 0);

    for (path, name) in [
        ("src/lib.rs", "rust_docced"),
        ("src/mod.ts", "tsDocced"),
        ("src/mod.py", "py_docced"),
    ] {
        let claim = handle
            .query(
                &QueryRequest::SearchSymbols {
                    query: Some(name.into()),
                    path_prefix: Some(path.into()),
                    kind: Some("function".into()),
                    include_tests: false,
                },
                QueryLimits::default(),
            )
            .unwrap();
        let QueryOutput::Symbols(symbols) = claim.value() else {
            panic!("symbols for {path}");
        };
        let symbol = symbols
            .iter()
            .find(|row| row.name == name)
            .unwrap_or_else(|| panic!("missing {name} in {path}: {symbols:?}"));
        assert!(
            symbol.doc_byte_start.is_some() && symbol.doc_byte_end.is_some(),
            "{path}::{name} must expose doc byte range, got {symbol:?}"
        );
        assert!(
            symbol.doc_byte_start.unwrap() < symbol.doc_byte_end.unwrap(),
            "doc range must be non-empty for {path}::{name}"
        );
        assert!(
            symbol.doc_byte_end.unwrap() <= symbol.byte_start,
            "doc range must precede symbol body for {path}::{name}"
        );
    }
}

#[test]
fn s5_symbol_diff_exposes_old_and_new_signatures_on_param_change() {
    let root = tempfile::tempdir().unwrap();
    git(root.path(), &["init"]);
    git(
        root.path(),
        &["config", "user.email", "symforge@example.invalid"],
    );
    git(root.path(), &["config", "user.name", "symforge"]);
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::write(
        root.path().join("src/lib.rs"),
        "pub fn reshape(a: u32) -> u32 { a }\n",
    )
    .unwrap();
    git(root.path(), &["add", "."]);
    git(root.path(), &["commit", "-m", "base"]);
    let base = String::from_utf8(
        symforge::process_util::hidden_command("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(root.path())
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_owned();

    fs::write(
        root.path().join("src/lib.rs"),
        "pub fn reshape(a: u32, b: u32) -> u32 { a + b }\n",
    )
    .unwrap();
    git(root.path(), &["add", "."]);
    git(root.path(), &["commit", "-m", "params"]);

    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(
            root.path().to_path_buf(),
        ))
        .unwrap();
    settle(&handle, 0);

    let claim = handle
        .query(
            &QueryRequest::Diff {
                path: "src/lib.rs".into(),
                from_ref: base,
                to_ref: Some("HEAD".into()),
            },
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::Diff(diff) = claim.value() else {
        panic!("diff result");
    };
    let change = diff
        .changes
        .iter()
        .find(|row| row.name == "reshape" && row.change == "modified")
        .unwrap_or_else(|| panic!("missing modified reshape: {:?}", diff.changes));
    let before = change
        .before_signature
        .as_deref()
        .expect("old-side signature");
    let after = change
        .after_signature
        .as_deref()
        .expect("new-side signature");
    assert!(before.contains("reshape(a: u32)"), "before={before}");
    assert!(after.contains("reshape(a: u32, b: u32)"), "after={after}");
    assert_ne!(before, after);
}

#[test]
fn s6_host_status_carries_typed_index_failure_when_publication_blocked() {
    // Closed enum shape: serialize/deserialize stays on the named cause variants.
    let encoded = serde_json::to_string(&HostIndexFailureCause::ParseFailed).unwrap();
    assert_eq!(encoded, "\"parse_failed\"");
    assert_eq!(
        serde_json::from_str::<HostIndexFailureCause>("\"publication_blocked\"").unwrap(),
        HostIndexFailureCause::PublicationBlocked
    );

    let root = tempfile::tempdir().unwrap();
    git2::Repository::init(root.path()).unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::write(root.path().join("src/lib.rs"), "pub fn ok() {}\n").unwrap();

    let owner = HostRuntimeOwner::new(ProcessIndexRuntime::acquire().unwrap());
    let room = HostRoom::open_in_runtime(
        &owner,
        HostRoomGrant::new(
            "s6".into(),
            root.path().to_path_buf(),
            "s6".into(),
            HostRights::read_only(),
        )
        .unwrap(),
        HostRoomConfig {
            room_id: "s6".into(),
            limits: HostLimits::default(),
        },
    )
    .unwrap();
    let control = room.control().unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    let status = loop {
        let HostResponse::Status(status) = room.dispatch(&HostRequest::Status, &control).unwrap()
        else {
            panic!("status reply");
        };
        if status.phase == HostPhase::Current {
            break status;
        }
        assert!(Instant::now() < deadline, "source did not become current");
        std::thread::sleep(Duration::from_millis(10));
    };
    // Healthy current publication: no typed failure invented from free-text.
    assert_eq!(status.index_failure, None);

    room.close(&control).unwrap();
}
