//! Feature 036 slice A1 review item 1: a `self::` / `super::` path is resolved against the
//! module of the file that contains it, never matched as a bare suffix.
#![cfg(feature = "server")]

mod reliability_support;

use reliability_support::{call, server_for};
use serde_json::json;

fn write(root: &std::path::Path, rel: &str, text: &str) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    std::fs::write(path, text).expect("write");
}

/// `a.rs` calls `self::helper()` (its own, absent, `helper`); `b.rs` defines `helper`;
/// `c.rs` calls `crate::b::helper()` (control: a real caller).
fn scenario() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    write(
        dir.path(),
        "src/lib.rs",
        "pub mod a;\npub mod b;\npub mod c;\n",
    );
    write(
        dir.path(),
        "src/a.rs",
        "pub fn run() {\n    self::helper();\n}\n",
    );
    write(dir.path(), "src/b.rs", "pub fn helper() {}\n");
    write(
        dir.path(),
        "src/c.rs",
        "pub fn go() {\n    crate::b::helper();\n}\n",
    );
    dir
}

#[tokio::test]
async fn self_path_in_another_file_is_not_a_caller() {
    let dir = scenario();
    let server = server_for(dir.path());
    let text = call(
        &server,
        "find_references",
        json!({ "name": "helper", "path": "src/b.rs" }),
    )
    .await;
    assert!(
        text.contains("src/c.rs"),
        "control: real caller missing\n{text}"
    );
    assert!(
        !text.contains("src/a.rs"),
        "`self::helper()` in a.rs is a.rs's own helper, not b.rs's\n{text}"
    );
}

#[tokio::test]
async fn self_path_in_another_file_is_not_a_dependent() {
    let dir = scenario();
    let server = server_for(dir.path());
    let text = call(&server, "find_dependents", json!({ "path": "src/b.rs" })).await;
    assert!(
        text.contains("src/c.rs"),
        "control: real dependent missing\n{text}"
    );
    assert!(
        !text.contains("src/a.rs"),
        "a.rs only has `self::helper()`; it does not depend on b.rs\n{text}"
    );
}

/// Delta b: a `self::` path that resolves to another module is a non-match for every kind,
/// not only functions (here a struct reached from a dependent file).
#[tokio::test]
async fn self_path_to_a_struct_elsewhere_is_not_a_reference() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(
        dir.path(),
        "src/lib.rs",
        "pub mod a;\npub mod b;\npub mod c;\n",
    );
    write(
        dir.path(),
        "src/a.rs",
        "use crate::b::helper_fn;\npub fn make() {\n    helper_fn();\n    let _ = self::Widget(1);\n}\n",
    );
    write(
        dir.path(),
        "src/b.rs",
        "pub struct Widget(pub u32);\npub fn helper_fn() {}\n",
    );
    write(
        dir.path(),
        "src/c.rs",
        "pub fn go() {\n    let _ = crate::b::Widget(2);\n}\n",
    );
    let server = server_for(dir.path());
    let text = call(
        &server,
        "find_references",
        json!({ "name": "Widget", "path": "src/b.rs" }),
    )
    .await;
    assert!(
        text.contains("src/c.rs"),
        "control: real user missing\n{text}"
    );
    assert!(
        !text.contains("src/a.rs"),
        "`self::Widget` in a.rs is not b.rs's Widget\n{text}"
    );
}
