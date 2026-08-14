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
//! STATED BOUND (round 7; hardened round 8): an inert `///` line does
//! not execute in the CRATE's compilation, but rustdoc extracts fenced
//! doc-comment text into separate doctest crates that plain `cargo test`
//! (or `--doc`) would build and RUN — an executing edge this sweep would
//! tolerate as prose. The bound holds because no gate here builds
//! doctests, and that is not left as a hand-checked snapshot:
//! `no_gate_builds_doctests` below scans every PHYSICAL LINE of every
//! `.yml`/`.yaml` workflow, erases YAML quoting and flow punctuation
//! before tokenizing, splits compound commands into segments, and
//! judges every segment naming `cargo`. Round 9 killed the previous
//! `run:`-scalar parser, which missed quoted scalars, plain multi-line
//! scalars, flow mappings, an extra space after the list dash, and the
//! `cargo t` alias: a scan that must model YAML to find the command
//! loses to YAML, exactly as the mid-line-comment lexers lost to Rust
//! in rounds 1–3. The rule is fail-closed instead: a `cargo` segment
//! must resolve to a subcommand ON THAT LINE — one that does not (any
//! line-spanning wrap, folded or shell-continued) is an OFFENSE, not a
//! skip — and a `test`/`t` subcommand must carry a doctest-excluding
//! selector before any bare `--` and must not carry `--doc`. STATED
//! RESIDUALS of the pin, and now the only two: a gate reaching cargo
//! through INDIRECTION (a script, make target, or composite action), and
//! a command assembled from YAML anchors or `${{ }}` expressions. Both
//! change what the runner executes without putting the command in any
//! line this scan reads.
//!
//! STATED RESIDUAL (C9 ruling): `include!`/`#[path]` can mount source
//! across directory boundaries. The mechanism sweep is a fail-closed
//! TRIPWIRE over known spellings, not a completeness proof. Every arm
//! judges four views of the line: the whitespace-collapsed form, the
//! collapsed form of the RAW line with `/*…*/` block-comment spans
//! removed DEPTH-AWARE (round 6: comments are token separators the
//! collapse never saw; round 7: they NEST, so removal counts depth;
//! round 8: stripping must run BEFORE the collapse — deleting whitespace
//! first fabricated `/*` openers out of spaced `/ *` — and must never
//! discard collected output — the dangling-`*/` clear wiped flagged
//! prefixes), and each of those with `r#` removed as EXTRA views, since
//! in-place removal could fabricate or destroy an adjacency. A match on
//! ANY view flags. A line carrying a `"` together with a block-comment
//! delimiter and a SPLICE TOKEN is flagged OUTRIGHT (round 8): string
//! content can poison any line-local comment tracking, so a line where
//! that poison could matter is surfaced, never judged. Round 9 measured
//! what that arm does and does NOT establish, and the honest statement
//! is narrower than round 8's: lines carrying a quote and a delimiter
//! but NO splice token still reach the views (two exist in `src/`
//! today), and a quote-free line can be the INTERIOR of a multi-line
//! string where a raw `/*` is content, not an opener — so the stripped
//! view's removals are not always real comment interior. Both errors
//! run in the OVER-flag direction only: an under-flag would need a live
//! single-line splice whose `include`/`path` token is hidden, and the
//! ambiguity arm tests for those tokens in RAW text, before any
//! stripping. That is the claim — a direction, not an exactness. The
//! arms flag: any `include!` spelling, any `include` at one of FOUR
//! enumerated openers on its declaration line — `::include`,
//! `{include`, `,include`, and `useinclude` after collapse — any
//! `#[path`/`path=` attribute spelling, and any U+200E/U+200F bidi mark
//! outright. The fourth opener is round 9's: Rust 2018 uniform paths
//! let `use include as mount;` bind the prelude macro with no leading
//! path, which is live on this crate's edition and wrote none of the
//! first three. The claim is the ENUMERATION, not a universal — round 9
//! falsified "the form every alias site must write, whatever its
//! visibility, spacing, grouping, comment interleaving, or `r#` raw
//! spelling", and an enumeration that has been widened four times is a
//! tripwire, not a proof. What escapes a line-based text scan
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
    // changing what the line does. So every arm judges FOUR views of the
    // line: the whitespace-collapsed form, the collapsed form of the RAW
    // line with block-comment spans stripped depth-aware (round 8: strip
    // BEFORE collapse, and never discard — see `strip_block_comments`),
    // and each of those with `r#` removed as extra views (a match on ANY
    // flags — over-flagging is safe friction, a missed adjacency is
    // not). Lines where a `"` coexists with a block-comment delimiter
    // and a splice token are flagged OUTRIGHT before any view is judged
    // (round 8): string content can poison the depth tracking. The two
    // Pattern_White_Space extras the Rust lexer also accepts (the
    // U+200E/U+200F bidi marks, the round-4 dodge) are flagged OUTRIGHT —
    // they have no legitimate use in this source. The alias arm (round 5,
    // after a use-prefix test and a raw word-boundary test each proved
    // wrong — the first evadable, the second flooded by prose) flags
    // `include` in a position where a use-declaration can bind it, at
    // FOUR enumerated openers on the collapsed line: `::include`,
    // `{include`, `,include`, and — round 9 — `useinclude`. That last
    // one is not a stylistic variant: Rust 2018 UNIFORM PATHS let a use
    // declaration name a prelude macro with no leading path at all, so
    // `use include as mount;` is a live single-line splice alias on this
    // crate's own edition (2024) and wrote none of the first three
    // openers. Round 9 proved it compiles AND executes, and proved the
    // sweeps could not tell clean HEAD from a HEAD carrying it. The claim
    // here is now the enumerated one — these four openers, not "whatever
    // an alias site must write" — because that universal quantifier is
    // what round 9 falsified. A declaration split across physical lines
    // remains the stated split residual of the file header.
    // The tail check is deliberately BROADER than "non-identifier or
    // glued `as`": end-of-line counts as boundary-clear, any non-ASCII
    // character counts as non-identifier, and any tail beginning `as`
    // matches — so a hypothetical `::includeastro` segment flags too.
    // All three widenings over-flag only, never under-flag.
    // `include_filtered`/`include_str!` carry a `_` at the boundary and
    // stay unmatched.
    fn names_include_segment(collapsed: &str) -> bool {
        for opener in ["::include", "{include", ",include", "useinclude"] {
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
    // Remove `/*…*/` spans DEPTH-AWARE from the RAW line — Rust block
    // comments nest (round 7), and on a quote-free line a raw-text `/*`
    // adjacency is a comment opener to the real lexer too. Round 8: this
    // must run BEFORE whitespace collapse (collapsing first glued spaced
    // `/ *` into openers the lexer never saw) and must NEVER discard
    // collected output (the old dangling-`*/` clear deleted an already
    // collected flagged prefix whenever a later string or trailing line
    // comment carried `*/`). A depth-0 `*/` is skipped and everything
    // kept — over-flag only; an unclosed `/*` drops the rest of the
    // line, which on a quote-free line is real comment interior.
    // Removing the delimiters can glue the surrounding text — that
    // fabrication over-flags only, never under-flags. Lines where string
    // content could poison this tracking (a `"` alongside a delimiter)
    // never reach the views at all: the ambiguity arm flags them first.
    fn strip_block_comments(raw: &str) -> String {
        let mut out = String::new();
        let mut depth = 0usize;
        let mut chars = raw.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '/' && chars.peek() == Some(&'*') {
                chars.next();
                depth += 1;
            } else if c == '*' && chars.peek() == Some(&'/') {
                chars.next();
                depth = depth.saturating_sub(1);
            } else if depth == 0 {
                out.push(c);
            }
        }
        out
    }
    let splice_matcher = |line: &str| -> Option<&'static str> {
        if line.contains('\u{200E}') || line.contains('\u{200F}') {
            return Some("bidi mark");
        }
        // Round 8, the ambiguity arm: string content can poison any
        // line-local comment tracking (a `"/*"` literal opens a span the
        // lexer never saw; a `"*/"` literal closes one it never opened),
        // and the poisoned stripped view can hide a live splice the
        // comment bytes simultaneously hide from the plain view. Such
        // lines are not judged — they are flagged outright whenever the
        // splice tokens are present at all. Round 9 pinned down what
        // this does NOT buy: lines with a quote and a delimiter but no
        // splice token still reach the views, and a quote-free line can
        // be the interior of a multi-line string, so a judged line's
        // delimiters are not always real. What holds is the DIRECTION —
        // a fake delimiter can only remove text and over-flag, while an
        // under-flag would need a live single-line splice whose
        // `include`/`path` token is hidden, and the test below runs on
        // RAW text before any stripping can hide one.
        if line.contains('"')
            && (line.contains("/*") || line.contains("*/"))
            && (line.contains("include")
                || (line.contains('#') && line.contains('[') && line.contains("path")))
        {
            return Some("comment/string ambiguity");
        }
        let collapse = |s: &str| -> String { s.chars().filter(|c| !c.is_whitespace()).collect() };
        let plain = collapse(line);
        let stripped = collapse(&strip_block_comments(line));
        // `r#`-removal is an EXTRA pair of views, not an in-place edit:
        // removing two bytes can fabricate or destroy an adjacency, and
        // an extra view can only ever ADD a flag (round 8).
        let views = [
            plain.replace("r#", ""),
            stripped.replace("r#", ""),
            plain,
            stripped,
        ];
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

#[test]
fn no_gate_builds_doctests() {
    // Round 7: rustdoc extracts fenced doc-comment text into doctest
    // crates that a bare `cargo test` (or `--doc`) builds and RUNS — an
    // executing edge the prose exemption above would tolerate. The
    // inert-comment rule is therefore bounded by the gates never opening
    // the doctest lane, and this test OBSERVES that bound instead of
    // asserting it from memory. Round 8 falsified the first scan and
    // round 9 falsified its replacement: a `run:`-scalar walk missed
    // quoted scalars (`run: "cargo test"` — the token was `"cargo`),
    // plain multi-line scalars, a flow mapping (`- {run: cargo test}`),
    // an extra space after the list dash, and the `cargo t` builtin
    // alias. Each shape was a real CI gate the walk read as nothing.
    //
    // The lesson is round 3's, arriving a second time: a scan that must
    // MODEL a syntax to find the command loses to that syntax. So this
    // no longer parses YAML at all. It is a fail-closed PHYSICAL-LINE
    // scan, and the rule is: a line that mentions `cargo` must resolve,
    // on that line alone, to a subcommand this test can name.
    //   * YAML/flow punctuation (`"`, `'`, `{`, `}`, `,`) becomes
    //     whitespace first, so a quoted scalar and a flow mapping
    //     tokenize exactly like the bare form. No quoting spelling can
    //     hide the command, because quoting is erased before tokens.
    //   * Each line splits into command segments on `&&`/`||`/`;`/`|`,
    //     so a sibling command's selector cannot mask a bare gate.
    //   * A segment holding the token `cargo` with NO subcommand token
    //     after it — the shape produced by every line-spanning wrap,
    //     YAML-folded or shell-continued — is an OFFENSE, not a skip.
    //     That is the fail-closed core: the walk refuses to guess what a
    //     wrapped command becomes, and says so loudly. A future gate
    //     that legitimately wraps must inline the command or update this
    //     test deliberately, which is the friction the whole file is
    //     built on.
    //   * A subcommand of `test` (or the `t` builtin alias) is an
    //     invocation. It must carry a doctest-excluding target selector
    //     BEFORE any bare `--` (after it, tokens belong to libtest — a
    //     trailing `--test` is a filter string, not a selector), and
    //     `--doc` anywhere in the segment is an offense.
    //     (`--test-threads` is a distinct token and does not satisfy
    //     `--test`.) Any other subcommand — `build`, `clippy`, `check`,
    //     `metadata`, `tree` — is not this test's business.
    // The residuals are now the honest two, and they are the two no
    // line-based scan can reach: indirection (a script, make target, or
    // composite action that runs cargo out of the walk's sight) and a
    // command assembled from YAML anchors or `${{ }}` expressions. Both
    // are STATED in the file header.
    let repo = src_root().parent().expect("src has a parent").to_path_buf();
    let workflows = repo.join(".github").join("workflows");
    let excluding = ["--all-targets", "--lib", "--tests", "--bins", "--test"];
    let mut invocations = 0usize;
    let mut offenders = Vec::new();
    for entry in std::fs::read_dir(&workflows).expect("read workflows dir") {
        let path = entry.expect("workflow entry").path();
        if path.extension().is_none_or(|e| e != "yml" && e != "yaml") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("read workflow");
        for (number, line) in text.lines().enumerate() {
            let unquoted = line.replace(['"', '\'', '{', '}', ','], " ");
            for segment in unquoted
                .replace("&&", "\n")
                .replace("||", "\n")
                .replace([';', '|'], "\n")
                .lines()
            {
                let tokens: Vec<&str> = segment.split_whitespace().filter(|t| *t != "\\").collect();
                let Some(cargo_at) = tokens.iter().position(|t| *t == "cargo") else {
                    continue;
                };
                let after = &tokens[cargo_at + 1..];
                let dashdash = after.iter().position(|t| *t == "--").unwrap_or(after.len());
                let mut report = |what: &str| {
                    offenders.push(format!(
                        "{}:{}: [{what}] {}",
                        path.file_name().expect("file name").to_string_lossy(),
                        number + 1,
                        segment.trim()
                    ));
                };
                let Some(subcommand) = after[..dashdash]
                    .iter()
                    .find(|t| !t.starts_with('-') && !t.starts_with('$'))
                else {
                    report("cargo invocation does not resolve on one line");
                    continue;
                };
                if *subcommand != "test" && *subcommand != "t" {
                    continue;
                }
                invocations += 1;
                if after.contains(&"--doc") {
                    report("--doc builds the doctest lane");
                } else if !after[..dashdash].iter().any(|t| excluding.contains(t)) {
                    report("no doctest-excluding target selector before `--`");
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "gate invocations that would build doctests (see the STATED BOUND in \
         this file's header — a doctest is an executing edge the prose \
         exemption tolerates):\n{}",
        offenders.join("\n")
    );
    assert!(
        invocations >= 5,
        "only {invocations} `cargo test` invocations found across the CI \
         workflows — the gate walk is broken or CI moved; update this test \
         with the workflows"
    );
}
