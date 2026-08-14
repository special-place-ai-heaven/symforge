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
//! The rule is fail-closed and LEXER-FREE (C8 ruling, second arm; narrowed
//! in round 6): a line mentioning a guarded surface passes only as an
//! INERT full-line comment — first non-whitespace bytes `//`, no `"` and
//! no `*/` anywhere on the line — or as an exactly-allowlisted line. Bare
//! `//` was not enough: Rust has exactly two lexeme classes that span
//! physical lines, string literals and block comments, and the tail line
//! of either may BEGIN with `//` as content yet hand control back to code
//! later on the same line. Handing back requires the closing delimiter on
//! that line — a `"` for every string form (plain, raw, byte, C), a `*/`
//! for every block-comment level — so a `//`-starting line free of both
//! cannot carry a call edge. Every mention on a CODE line, and every
//! quote- or `*/`-bearing comment line, FAILS and forces a human
//! decision. Rounds 1–3 proved every attempted mid-line-comment lexer
//! laundered a call edge through some literal form; this rule has no
//! lexer to be wrong.
//!
//! STATED RESIDUAL (C9 ruling): `include!`/`#[path]` can mount source
//! across directory boundaries. The mechanism sweep is a fail-closed
//! TRIPWIRE over known spellings, not a completeness proof. Every arm
//! judges two views of the line — whitespace-and-`r#`-collapsed, and that
//! view with `/*…*/` block-comment spans removed (round 6: comments are
//! token separators the whitespace collapse never saw) — and flags: any
//! `include!` spelling, any `include` in path-segment position on its
//! declaration line (`::include`/`{include`/`,include` after collapse —
//! the form every single-line alias-creation site must write, whatever
//! its visibility, spacing, grouping, comment interleaving, or `r#` raw
//! spelling), any `#[path`/`path=` attribute spelling, and any
//! U+200E/U+200F bidi mark outright. What escapes a line-based text scan
//! by construction: a declaration or invocation split across physical
//! lines (a block comment or string spanning the boundary is the same
//! class), a `concat!`/`env!("OUT_DIR")` argument naming the dark
//! directory without its token — including the COMPOUND of the two, a
//! split invocation carrying a concat argument, which no single line of
//! this scan can see — and any future spelling not enumerated here. The
//! load-bearing darkness guarantee is NOT this tripwire — it is the
//! inert-comment rule of the sweeps above, applied to every line that
//! lives in `src/`.

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
/// comment exception; round 6: close the spanning-lexeme tail). Three
/// rounds of review proved a mid-line-comment lexer is an arms race this
/// file cannot win — string literals, then char literals, then raw-string
/// quote parity each laundered a call edge. So there is no lexer and no
/// mid-line tolerance at all: a line is prose ONLY when its first
/// non-whitespace bytes are `//` AND it contains neither `"` nor `*/`.
/// Round 6 refuted the bare `//` form: a line-spanning string literal (or
/// block comment) can place `//` at line start as CONTENT and still
/// execute code after its closing delimiter on the same line. But that
/// closing delimiter must be present — every string form closes with a
/// `"`, every block-comment level with `*/` — so a line free of both is
/// inert end to end. A trailing comment, a quoting comment, or a comment
/// naming `*/` that mentions a guarded surface is treated as code and
/// must be allowlisted, safe friction.
fn is_inert_full_line_comment(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("//") && !line.contains('"') && !line.contains("*/")
}

struct Sweep {
    violations: Vec<String>,
    allowlisted_seen: Vec<(&'static str, &'static str)>,
    prose_lines: usize,
    files_scanned: usize,
}

/// `matches_line` names the guarded pattern a line contains, or `None`. A
/// matched line passes only as an INERT full-line comment or by exact
/// allowlist.
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
            if is_inert_full_line_comment(line) {
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
            // Round 6: quote-bearing comment lines lost the prose
            // exemption (a `"` can be a spanning-string tail handing
            // control back to code), so the two legitimate quoting
            // comments are decided here instead of silently tolerated.
            (
                "src/lib.rs",
                "// call edge into index_lifecycle. Do NOT \"tidy\" either gate before the cut.",
            ),
            (
                "src/lifecycle_identity.rs",
                "//!     darkness as \"`grep -rn index_lifecycle src/` returns no hit outside it\".",
            ),
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
    // Round 5 (blocker): BOTH counts bind. Distinct alone let an exact
    // DUPLICATE of an allowlisted line — a second mount of the dark
    // directory under an innocuous alias — be absorbed silently; total
    // alone could not tell a deletion masked by a duplicate elsewhere.
    let seen: std::collections::BTreeSet<_> = result.allowlisted_seen.iter().collect();
    assert_eq!(
        (result.allowlisted_seen.len(), seen.len()),
        (4, 4),
        "the live_index mount pair plus two quote-bearing prose comments: \
         exactly four allowlisted lines, each seen exactly once; a duplicate \
         mount or a moved/reworded line must update this test deliberately"
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
            // Round 6: the quote-narrowed prose exemption surfaces the one
            // quote-bearing doc comment naming the module.
            (
                "src/index_lifecycle/public_api.rs",
                "/// * `\"keyword-flip\"` — `server_api`: a real `pub(crate)` module whose",
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
        (result.allowlisted_seen.len(), seen.len()),
        (9, 9),
        "one lib.rs declaration, seven wrap-table/delta string lines, and one \
         quote-bearing doc comment, each seen EXACTLY ONCE; a duplicate or an \
         edit to any of them updates this allowlist deliberately, got: {:?}",
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
    // Rounds 3–6: whitespace, block comments, and the `r#` raw-identifier
    // prefix are all insignificant separators inside macro invocations,
    // paths, and attributes — the Rust lexer discards each without
    // changing what the line does. So every arm judges TWO views of the
    // line: the whitespace-and-`r#`-collapsed form, and that form with
    // `/*…*/` block-comment spans removed (a match on EITHER flags —
    // over-flagging is safe friction, a missed adjacency is not). The two
    // Pattern_White_Space extras the Rust lexer also accepts (the
    // U+200E/U+200F bidi marks, the round-4 dodge) are flagged OUTRIGHT —
    // they have no legitimate use in this source. The alias arm (round 5,
    // after a use-prefix test and a raw word-boundary test each proved
    // wrong — the first evadable, the second flooded by prose) flags
    // `include` in PATH-SEGMENT position, which every SINGLE-LINE
    // alias-creation site must write at its first hop from the std/core
    // root, whatever its visibility, spacing, grouping, comment
    // interleaving, or `r#` spelling; a declaration split across physical
    // lines is the stated split residual of the file header.
    // The macro name `include` in path-segment position: preceded by `::`,
    // `{`, or `,` on the collapsed line. The tail check is deliberately
    // BROADER than "non-identifier or glued `as`": end-of-line counts as
    // boundary-clear, any non-ASCII character counts as non-identifier,
    // and any tail beginning `as` matches — so a hypothetical
    // `::includeastro` segment flags too. All three widenings over-flag
    // only, never under-flag. `include_filtered`/`include_str!` carry a
    // `_` at the boundary and stay unmatched.
    fn names_include_segment(collapsed: &str) -> bool {
        for opener in ["::include", "{include", ",include"] {
            let mut search_from = 0;
            while let Some(position) = collapsed[search_from..].find(opener) {
                let end = search_from + position + opener.len();
                let tail = &collapsed[end..];
                let boundary_clear = !tail
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
                if boundary_clear || tail.starts_with("as") {
                    return true;
                }
                search_from = end;
            }
        }
        false
    }
    // Remove minimal `/*…*/` spans repeatedly; an unclosed `/*` comments
    // out the rest of the line, a dangling `*/` comments out the start.
    // Over-removal on pathological string content is harmless because the
    // arms also run on the unstripped view.
    fn strip_block_comments(collapsed: &str) -> String {
        let mut out = collapsed.to_string();
        while let Some(open) = out.find("/*") {
            match out[open + 2..].find("*/") {
                Some(close) => out.replace_range(open..open + 2 + close + 2, ""),
                None => {
                    out.truncate(open);
                    break;
                }
            }
        }
        if let Some(close) = out.find("*/") {
            out.replace_range(..close + 2, "");
        }
        out
    }
    let splice_matcher = |line: &str| -> Option<&'static str> {
        if line.contains('\u{200E}') || line.contains('\u{200F}') {
            return Some("bidi mark");
        }
        let plain: String = line
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
            .replace("r#", "");
        let stripped = strip_block_comments(&plain).replace("r#", "");
        let views = [plain.as_str(), stripped.as_str()];
        if views.iter().any(|v| v.contains("include!")) {
            return Some("include!");
        }
        if views.iter().any(|v| names_include_segment(v)) {
            return Some("include path segment");
        }
        if views.iter().any(|v| v.contains("#[path")) {
            return Some("#[path");
        }
        if views
            .iter()
            .any(|v| v.contains("path=") && v.contains("#["))
        {
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
            // Round 6: this comment QUOTES a `#[path]` spelling, so the
            // quote-narrowed prose exemption no longer covers it.
            (
                "src/protocol/format.rs",
                "// rather than recalled: `#[path = \"../claim_provenance.rs\"]` makes it look for",
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
        (result.allowlisted_seen.len(), seen.len()),
        (6, 6),
        "three test-fixture include!(concat!( sites, two #[path] mounts, and \
         one quoting comment, each seen EXACTLY ONCE as a (file, line) \
         allowlist entry; a duplicate or a new splice site is a deliberate \
         allowlist change, got: {:?}",
        result.allowlisted_seen
    );
}
