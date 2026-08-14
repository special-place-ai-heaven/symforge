//! Feature 020 V11, T051 — the call-edge proof for the dark Slice 3 runtime.
//!
//! `src/index_lifecycle/mod.rs` states the darkness property as CALL EDGES,
//! not grep hits: no code outside that directory names an item in it; the
//! `#[path]` mount in `live_index/mod.rs` is a declaration, not a call edge;
//! prose mentions are not edges either. This file formalizes that paragraph
//! into an executing test, per lane: the daemon, stdio, serve, embed,
//! snapshot, observer, and mutation entry points are all `src/` production
//! code, so one sweep over `src/` minus the dark directory covers every one
//! of them — and the lane roots are asserted to EXIST so a moved file cannot
//! make the claim vacuously true.
//!
//! The rule is deliberately fail-closed, no lexer: a line mentioning the
//! dark surface passes only if the mention sits after `//` on its own line
//! (prose) or the line is one of the exactly-known mount declarations. A
//! new string literal or block comment naming the surface in production code
//! will FAIL this test and force a human decision — safe friction, never a
//! silent tolerance. Real call edges (`use`/qualified paths) always carry
//! the token before any `//` and cannot pass.

use std::path::{Path, PathBuf};

fn src_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn rust_files_under(root: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(root).expect("read src directory") {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            rust_files_under(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// A line passes for `token` when every occurrence sits after a `//` — the
/// prose form — so comments and doc comments are tolerated and code is not.
fn only_in_line_comment(line: &str, token: &str) -> bool {
    match line.find("//") {
        Some(comment_start) => !line[..comment_start].contains(token),
        None => false,
    }
}

struct Sweep {
    violations: Vec<String>,
    allowlisted_seen: Vec<&'static str>,
    prose_lines: usize,
    files_scanned: usize,
}

fn sweep(
    token: &str,
    exclude_dir: &Path,
    exclude_file: Option<&Path>,
    allowlist: &[(&'static str, &'static str)],
) -> Sweep {
    let src = src_root();
    let mut files = Vec::new();
    rust_files_under(&src, &mut files);
    let mut result = Sweep {
        violations: Vec::new(),
        allowlisted_seen: Vec::new(),
        prose_lines: 0,
        files_scanned: 0,
    };
    for file in &files {
        if file.starts_with(exclude_dir) || exclude_file.is_some_and(|f| file == f) {
            continue;
        }
        result.files_scanned += 1;
        let text = std::fs::read_to_string(file).expect("read source file");
        let display = file
            .strip_prefix(src.parent().expect("src has a parent"))
            .expect("file under repo")
            .to_string_lossy()
            .replace('\\', "/");
        for (number, line) in text.lines().enumerate() {
            if !line.contains(token) {
                continue;
            }
            if let Some((_, allowed)) = allowlist
                .iter()
                .find(|(f, l)| *f == display && *l == line.trim())
            {
                result.allowlisted_seen.push(allowed);
                continue;
            }
            if only_in_line_comment(line, token) {
                result.prose_lines += 1;
                continue;
            }
            result
                .violations
                .push(format!("{display}:{}: {}", number + 1, line.trim()));
        }
    }
    result
}

/// The seven ingress lanes the task names, as paths that must EXIST — if one
/// moves, this test fails loudly instead of proving a claim about nothing.
const INGRESS_LANES: &[(&str, &str)] = &[
    ("daemon", "src/daemon.rs"),
    ("stdio", "src/protocol"),
    ("serve", "src/server"),
    ("embed", "src/embed.rs"),
    ("snapshot", "src/live_index/persist.rs"),
    ("observer", "src/watcher"),
    ("mutation", "src/protocol/edit_tools.rs"),
];

#[test]
fn the_dark_directory_has_no_call_edge_from_any_production_lane() {
    let repo = src_root().parent().expect("src has a parent").to_path_buf();
    for (lane, path) in INGRESS_LANES {
        assert!(
            repo.join(path).exists(),
            "ingress lane `{lane}` no longer lives at {path}; the sweep below \
             would be claiming unreachability for a lane it cannot see — move \
             this entry with the code"
        );
    }

    let result = sweep(
        "index_lifecycle",
        &src_root().join("index_lifecycle"),
        None,
        &[
            (
                "src/live_index/mod.rs",
                "#[path = \"../index_lifecycle/mod.rs\"]",
            ),
            ("src/live_index/mod.rs", "pub mod index_lifecycle;"),
        ],
    );

    assert!(
        result.violations.is_empty(),
        "call edges into the dark directory from production code:\n{}",
        result.violations.join("\n")
    );
    // Anti-vacuity: the sweep must have SEEN what the tree is known to hold —
    // both halves of the one permitted mount, the prose mentions that prove
    // comments are tolerated rather than never encountered, and a file count
    // that says the walk actually walked.
    assert_eq!(
        result.allowlisted_seen.len(),
        2,
        "the live_index mount declaration is the ONE permitted code mention, \
         in exactly two lines; a moved or reworded mount must update this test \
         deliberately"
    );
    assert!(
        result.prose_lines > 0,
        "the tree is known to mention the dark directory in prose (e.g. \
         lifecycle_identity.rs docs); zero tolerated prose lines means the \
         comment rule never exercised"
    );
    assert!(
        result.files_scanned > 100,
        "only {} files scanned — the source walk is broken",
        result.files_scanned
    );
}

#[test]
fn the_flip_ready_module_is_declared_once_and_never_called() {
    // `server_api::run` staying uncalled is the SIBLING assertion to the
    // directory sweep above, not a substitute for it. The dark directory is
    // excluded here by transitivity: it is proven unreachable by the sibling
    // test, so nothing inside it can be a production ingress to the stub —
    // its wrap-table STRINGS name the atoms without calling anything.
    let result = sweep(
        "server_api",
        &src_root().join("index_lifecycle"),
        Some(&src_root().join("server_api.rs")),
        &[("src/lib.rs", "pub(crate) mod server_api;")],
    );

    assert!(
        result.violations.is_empty(),
        "production references to the flip-ready module:\n{}",
        result.violations.join("\n")
    );
    // The declaration must be FOUND, and found in its pub(crate) form: a
    // premature keyword flip changes the line, drops it from the allowlist,
    // and fails this test — activation flips the keyword AND this pin in the
    // same deliberate change, never as a tidy-up.
    assert_eq!(
        result.allowlisted_seen,
        vec!["pub(crate) mod server_api;"],
        "lib.rs no longer declares server_api as pub(crate); if this is the \
         activation cut, update this pin in the same change — if it is not, \
         the census just widened by four atoms"
    );
}
