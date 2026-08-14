//! Feature 020 V11, T051 — the call-edge proof for the dark Slice 3 runtime.
//!
//! `src/index_lifecycle/mod.rs` states the darkness property as CALL EDGES,
//! not grep hits: no code outside that directory names an item in it; the
//! `#[path]` mount in `live_index/mod.rs` is a declaration, not a call edge;
//! prose mentions are not edges either. This file formalizes that paragraph
//! into executing tests, per lane: the daemon, stdio, serve, embed,
//! snapshot, observer, and mutation entry points are all `src/` production
//! code, so one sweep over `src/` minus the dark directory covers every one
//! of them — and the lane roots are asserted to EXIST so a moved file cannot
//! make the claim vacuously true.
//!
//! The rule is fail-closed, no full lexer: a line mentioning a guarded
//! surface passes only as prose (the token strictly after a `//` that sits
//! OUTSIDE any string literal — C8 ruling) or as an exactly-allowlisted
//! line. A novel string literal or block-comment mention FAILS and forces a
//! human decision — safe friction, never silent tolerance.
//!
//! STATED RESIDUAL (C9 ruling): `include!`/`#[path]` can mount source across
//! directory boundaries, and a `concat!`/`env!("OUT_DIR")` argument can name
//! the dark directory without ever writing its token — that construction is
//! uncatchable by any token scan, and this file does not claim to catch it.
//! What it does instead is fail closed on the MECHANISMS: every `include!`
//! and `#[path]` in `src/` must be on the exact allowlist below, so a new
//! splice site cannot appear silently.

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

/// Byte offset of the first `//` that sits OUTSIDE any string literal (C8:
/// a `//` inside a string must not launder the rest of the line as prose).
/// The tracker is deliberately conservative: a `'"'` char literal flips the
/// string state wrongly, which can only HIDE a comment start — the token is
/// then treated as code and flagged, never silently tolerated.
fn comment_start_outside_strings(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    let mut in_string = false;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if in_string => i += 1,
            b'"' => in_string = !in_string,
            b'/' if !in_string && i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                return Some(i);
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// A line passes for `token` when every occurrence sits after a real `//`.
fn only_in_line_comment(line: &str, token: &str) -> bool {
    match comment_start_outside_strings(line) {
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
    tokens: &[&str],
    exclude_dir: Option<&Path>,
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
        if exclude_dir.is_some_and(|d| file.starts_with(d))
            || exclude_file.is_some_and(|f| file == f)
        {
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
            let Some(token) = tokens.iter().find(|t| line.contains(**t)) else {
                continue;
            };
            if let Some((_, allowed)) = allowlist
                .iter()
                .find(|(f, l)| *f == display && *l == line.trim())
            {
                result.allowlisted_seen.push(allowed);
                continue;
            }
            if tokens.iter().all(|t| only_in_line_comment(line, t)) {
                result.prose_lines += 1;
                continue;
            }
            result.violations.push(format!(
                "{display}:{}: [{token}] {}",
                number + 1,
                line.trim()
            ));
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
        &["index_lifecycle"],
        Some(&src_root().join("index_lifecycle")),
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
    // directory sweep above, not a substitute for it. C10 ruling: the dark
    // directory is swept too — its wrap-table STRING lines are allowlisted
    // individually below, so a real `use`/call edge from `index_lifecycle`
    // into the stub cannot hide behind a directory exemption.
    let result = sweep(
        &["server_api"],
        None,
        Some(&src_root().join("server_api.rs")),
        &[
            ("src/lib.rs", "pub(crate) mod server_api;"),
            (
                "src/index_lifecycle/public_api.rs",
                "atom: \"symforge::server_api\",",
            ),
            (
                "src/index_lifecycle/public_api.rs",
                "atom: \"symforge::server_api::ServerBootstrapError\",",
            ),
            (
                "src/index_lifecycle/public_api.rs",
                "atom: \"symforge::server_api::ServerExit\",",
            ),
            (
                "src/index_lifecycle/public_api.rs",
                "atom: \"symforge::server_api::run\",",
            ),
            ("src/index_lifecycle/public_api.rs", "\"server_api\": {"),
            (
                "src/index_lifecycle/public_api.rs",
                "\"form\": \"cfg feature=server gated pub(crate) mod server_api in src/lib.rs, std-only stub\",",
            ),
            (
                "src/index_lifecycle/public_api.rs",
                "\"activation\": \"one keyword behind the already-present server cfg gate: pub(crate) becomes pub, and the census gains the four server_api atoms in server graphs only - the embed-v11 projection excludes this module, so no embed cell may ever grow them\"",
            ),
        ],
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
    assert!(
        result
            .allowlisted_seen
            .contains(&"pub(crate) mod server_api;"),
        "lib.rs no longer declares server_api as pub(crate); if this is the \
         activation cut, update this pin in the same change — if it is not, \
         the census just widened by four atoms"
    );
    // The wrap-table string lines must ALL have been seen: an edited atom
    // string falls off the allowlist and fails above, and a silently deleted
    // one fails here.
    assert_eq!(
        result.allowlisted_seen.len(),
        8,
        "one lib.rs declaration plus seven wrap-table/delta string lines; an \
         edit to any of them updates this allowlist deliberately, got: {:?}",
        result.allowlisted_seen
    );
}

#[test]
fn source_splicing_is_allowlisted() {
    // C9 ruling: fail closed on the splice MECHANISMS across all of src/,
    // the dark directory included. The residual — a concat!-constructed
    // path — is stated in the file header, not silently absorbed.
    let result = sweep(
        &["include!(", "#[path"],
        None,
        None,
        &[
            ("src/live_index/coupling/lifecycle.rs", "include!(concat!("),
            ("src/live_index/coupling/walker.rs", "include!(concat!("),
            ("src/live_index/persist.rs", "include!(concat!("),
            (
                "src/live_index/mod.rs",
                "#[path = \"../index_lifecycle/mod.rs\"]",
            ),
            (
                "src/protocol/format.rs",
                "#[path = \"claim_provenance.rs\"]",
            ),
        ],
    );

    assert!(
        result.violations.is_empty(),
        "unallowlisted source-splice mechanisms:\n{}",
        result.violations.join("\n")
    );
    assert_eq!(
        result.allowlisted_seen.len(),
        5,
        "three test-fixture include!(concat!( sites and two #[path] mounts; \
         a new splice site is a deliberate allowlist change, got: {:?}",
        result.allowlisted_seen
    );
}
