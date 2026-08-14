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
//! The rule is fail-closed and LEXER-FREE (C8 ruling, second arm): a line
//! mentioning a guarded surface passes only as a FULL-LINE comment — first
//! non-whitespace bytes `//`, after which Rust permits no code on the line
//! — or as an exactly-allowlisted line. Every mention on a CODE line,
//! string literals and trailing comments included, FAILS and forces a
//! human decision. Rounds 1–3 proved every attempted mid-line-comment
//! lexer laundered a call edge through some literal form; this rule has no
//! lexer to be wrong, and round 4's adversarial attack on it was refuted.
//!
//! STATED RESIDUAL (C9 ruling): `include!`/`#[path]` can mount source
//! across directory boundaries. The mechanism sweep is a fail-closed
//! TRIPWIRE over known spellings, not a completeness proof: it names any
//! `include!` invocation (judged with the LEXER'S whitespace removed, bidi
//! marks flagged outright), any use-declaration aliasing `include`, any
//! `#[path` attribute head, and any attribute line carrying a `path=`
//! argument. What escapes a line-based text scan by construction: an
//! invocation split across lines, a `concat!`/`env!("OUT_DIR")` argument
//! naming the dark directory without its token, an alias INVOCATION site
//! (its mandatory creation site in `src/` is what trips), and any future
//! spelling not enumerated here. The load-bearing darkness guarantee is
//! NOT this tripwire — it is the full-line-comment rule of the sweeps
//! above, applied to every line that lives in `src/`.

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

/// The prose rule, made unarguable (C8 ruling's second arm: drop the
/// comment exception). Three rounds of review proved a mid-line-comment
/// lexer is an arms race this file cannot win — string literals, then char
/// literals, then raw-string quote parity each laundered a call edge. So
/// there is no lexer and no mid-line tolerance at all: a line is prose ONLY
/// when its first non-whitespace bytes are `//`. Rust cannot place code
/// after a line-start `//` on the same line, so a real call edge can never
/// satisfy this predicate — a trailing comment that mentions a guarded
/// surface is treated as code and must be allowlisted, safe friction.
fn is_full_line_comment(line: &str) -> bool {
    line.trim_start().starts_with("//")
}

struct Sweep {
    violations: Vec<String>,
    allowlisted_seen: Vec<(&'static str, &'static str)>,
    prose_lines: usize,
    files_scanned: usize,
}

/// `matches_line` names the guarded pattern a line contains, or `None`. A
/// matched line passes only as a FULL-LINE comment or by exact allowlist.
fn sweep(
    matches_line: &dyn Fn(&str) -> Option<&'static str>,
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
            let Some(token) = matches_line(line) else {
                continue;
            };
            if let Some(entry) = allowlist
                .iter()
                .find(|(f, l)| *f == display && *l == line.trim())
            {
                result.allowlisted_seen.push(*entry);
                continue;
            }
            if is_full_line_comment(line) {
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

fn contains_token(token: &'static str) -> impl Fn(&str) -> Option<&'static str> {
    move |line: &str| line.contains(token).then_some(token)
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
        &contains_token("index_lifecycle"),
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
    let seen: std::collections::BTreeSet<_> = result.allowlisted_seen.iter().collect();
    assert_eq!(
        seen.len(),
        2,
        "the live_index mount declaration is the ONE permitted code mention, \
         in exactly two DISTINCT lines; a moved or reworded mount must update \
         this test deliberately"
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
        &contains_token("server_api"),
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
            .contains(&("src/lib.rs", "pub(crate) mod server_api;")),
        "lib.rs no longer declares server_api as pub(crate); if this is the \
         activation cut, update this pin in the same change — if it is not, \
         the census just widened by four atoms"
    );
    // The wrap-table string lines must ALL have been seen: an edited atom
    // string falls off the allowlist and fails above, and a silently deleted
    // one fails here.
    let seen: std::collections::BTreeSet<_> = result.allowlisted_seen.iter().collect();
    assert_eq!(
        seen.len(),
        8,
        "one lib.rs declaration plus seven wrap-table/delta string lines, each \
         seen as a DISTINCT allowlist entry; an edit to any of them updates \
         this allowlist deliberately, got: {:?}",
        result.allowlisted_seen
    );
}

#[test]
fn source_splicing_is_allowlisted() {
    // C9 ruling: fail closed on the splice MECHANISMS across all of src/,
    // the dark directory included. Round 2 proved the first token set was
    // evadable by invocation form (`include! {`, `#[cfg_attr(..., path =`),
    // so the matcher now names: any `include!` regardless of delimiter, any
    // `#[path` attribute head, and any line carrying a `path =`/`path=`
    // argument inside an attribute. The residuals — a concat!-constructed
    // path, and an attribute form matching none of these spellings — are
    // stated in the file header, not silently absorbed.
    // Rounds 3–4: whitespace is insignificant in macro invocations and
    // attributes, so the matcher judges the line with the LEXER'S
    // whitespace set removed — `char::is_whitespace` is Unicode
    // White_Space, but Rust lexes Pattern_White_Space, which additionally
    // holds the U+200E/U+200F bidi marks (the round-4 dodge). Those two
    // are also flagged OUTRIGHT: they have no legitimate use in this
    // source. Round 4 further proved `use std::include as inc;` an
    // all-ASCII alias route, so any use-declaration naming `include` is
    // flagged at the alias-creation site.
    let splice_matcher = |line: &str| -> Option<&'static str> {
        if line.contains('\u{200E}') || line.contains('\u{200F}') {
            return Some("bidi mark");
        }
        let collapsed: String = line
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '\u{200E}' && *c != '\u{200F}')
            .collect();
        if collapsed.contains("include!") {
            return Some("include!");
        }
        let trimmed = line.trim_start();
        if (trimmed.starts_with("use ") || trimmed.starts_with("pub use "))
            && collapsed.contains("include")
        {
            return Some("use ...include alias");
        }
        if collapsed.contains("#[path") {
            return Some("#[path");
        }
        if collapsed.contains("path=") && collapsed.contains("#[") {
            return Some("attribute path=");
        }
        None
    };
    let result = sweep(
        &splice_matcher,
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
    let seen: std::collections::BTreeSet<_> = result.allowlisted_seen.iter().collect();
    assert_eq!(
        seen.len(),
        5,
        "three test-fixture include!(concat!( sites and two #[path] mounts, \
         each seen as a DISTINCT (file, line) allowlist entry; a new splice \
         site is a deliberate allowlist change, got: {:?}",
        result.allowlisted_seen
    );
}
