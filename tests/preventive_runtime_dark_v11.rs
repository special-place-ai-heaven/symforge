//! Feature 020 V11, T051 — the reviewed darkness baseline and caller tripwires.
//!
//! `src/index_lifecycle/mod.rs` states the darkness property as CALL EDGES,
//! not grep hits: no code outside that directory names an item in it; the
//! `#[path]` mount in `live_index/mod.rs` is a declaration, not a call edge;
//! prose mentions are not edges either. This file formalizes that paragraph
//! into executing tests, per lane: the daemon, stdio, serve, embed,
//! snapshot, observer, and mutation entry points are all `src/` production
//! code. The sweep reads every regular file below `src/`, not only `.rs`, and
//! the root manifest's exact explicit lib/bin topology is pinned and confined
//! canonically below that same root, so an extensionless or relocated Cargo
//! target cannot escape it. The lane roots are also asserted to EXIST so a
//! moved file cannot make the claim vacuously true. Rust name resolution is
//! not reconstructed here. Instead, the reviewed baseline establishes that the two excluded
//! implementation surfaces contain no pre-existing outward dispatch bridge;
//! an exact source-set seal below makes any later trait, inherent-method,
//! registration, or re-export bridge inside those surfaces fail, while the
//! lexical sweeps keep direct outside callers diagnostic. This is reviewed
//! baseline preservation, not a general compiler call-graph oracle.
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
//! `no_gate_builds_doctests` below PINS every line of every
//! `.yml`/`.yaml` workflow that mentions cargo OR rustdoc —
//! case-insensitively — against a verbatim allowlist a human judged
//! doctest-free, binds each line's OCCURRENCE COUNT plus the total, the
//! distinct set and the workflow-file count, pins the root Cargo config
//! verbatim, pins the production lib/bin topology, and forbids any other
//! config found by the bounded walk.
//! "Committable" here means visible to a normal `git add`; force-added
//! ignored paths are outside the bound, like configs outside the repo.
//! Round 16 corrected the walk's skip list: `.gitignore`'s `/target` is
//! ROOT-ANCHORED, so a nested `target/` can be committable and is now
//! WALKED. `.git` metadata is never entered; root `target` and any
//! `node_modules` are skipped only when `git check-ignore` confirms the
//! directory is excluded, and a literal case-insensitive `git ls-files`
//! pathspec must show no tracked path below it. The exact `/target` and
//! `node_modules/` rules remain human-readable
//! pins, while Git decides the effective normal-add boundary. The round-15
//! claim that all three were skipped for one reason — "nothing placed
//! there can be committed" — was false for `target`, and hid a
//! committable config one directory from the round-14 exploit path. The
//! guard also fingerprints each selected lower-case `.yml`/`.yaml` workflow
//! file whole. Rounds 8–11 each falsified a scan
//! that tried to read the command out of the file and judge it; round
//! 11's `cargo rustdoc -- --test` builds and runs doctests while naming
//! neither `test` nor `--doc` where the walk looked, and
//! `cargo te"st" --doc` was split by the scan's own quote-erasure into
//! words the shell joins. The pin recognizes LINES, not commands: an
//! unrecognized one fails whatever it says. Round 11 wrote "there is no
//! word model left to be wrong about" and round 13 falsified it —
//! normalization IS a word model, and `split_whitespace` (Unicode
//! White_Space) disagreed with bash's IFS, so a line carrying U+00A0
//! normalized onto a pinned gate that bash would have run differently.
//! Splitting is now ASCII space and tab, which is what bash splits on.
//! The per-line counts are round 12's: `(total, distinct)` binds a
//! bijection only when the two are EQUAL, and at `(30, 26)` a
//! compensated edit deleted a real test gate while both numbers held.
//! KNOWN RESIDUALS of the pin — what has been probed, NOT a proof of
//! exhaustiveness, because round 9 called its list "the only two",
//! round 10 produced a third, round 11 a fourth, round 12 showed one of
//! them was never a residual at all, and round 13 broke the check that
//! replaced it, and round 14 walked through the seam between a LINE and
//! the YAML SCALAR that actually executes: a gate reaching the doctest
//! lane from OUTSIDE these files (a script, make target, or composite
//! action running it out of sight), and a cargo config outside the repo
//! (`~/.cargo/config.toml`, `$CARGO_HOME`, or an ancestor of the
//! checkout) or below an existing `.cargo` directory (which Cargo reads
//! only when that outer `.cargo` is itself the working directory; no CI
//! gate does that). The walk also deliberately enters ignored trees
//! other than its three named skips, so configs under `/target-*/`,
//! `/.*/`, `**/.symforge/`, `/mcps/`, or `spacetime/*/target/` over-flag.
//! Two former residuals are now covered rather than conceded: every
//! normally add-visible `.cargo` config relevant to an in-repo,
//! non-`.cargo` working directory is pinned (round 14 —
//! cargo merges configs from the working directory upward, so a
//! descendant config plus `working-directory:` re-pointed a gate and
//! ran a `///` line's doctest into the dark directory), and a gate
//! DISABLED with `if: false` changes no cargo line but does change the
//! file, which the fingerprints see.
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
//! but NO splice token still reach the views (76 of them at 0f41db7f,
//! counted with the arm's own predicate — the "two" written here in
//! round 9 was asserted, not measured, in the very paragraph added to
//! replace an overclaim with a measurement), and a quote-free line can
//! be the INTERIOR of a multi-line
//! string where a raw `/*` is content, not an opener — so the stripped
//! view's removals are not always real comment interior. Both errors
//! run in the OVER-flag direction only: an under-flag would need a live
//! single-line splice whose `include`/`path` token is hidden, and the
//! ambiguity arm tests for those tokens in RAW text, before any
//! stripping. That is the claim — a direction, not an exactness. The
//! arms flag: any `include!` spelling, any `include` at one of FOUR
//! enumerated openers on its declaration line — `::include`,
//! `{include`, `,include`, and `useinclude` after collapse — and any
//! `#[path`/`path=` attribute spelling. (U+200E/U+200F bidi marks are
//! flagged outright too, but NOT by an arm of this tripwire: round 12
//! moved that decision into `sweep`, ahead of the allowlist and the
//! prose exemption, because a matcher that merely names a mark still
//! lets the exemption forgive it. This paragraph listed it as an arm
//! until round 15.) The fourth opener is round 9's: Rust 2018 uniform paths
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
//! splice tripwire is therefore not the load-bearing mechanism by itself.
//! Darkness is held by the reviewed whole-`src` seal. The narrower
//! excluded-source seal and outside caller sweeps remain diagnostic tripwires:
//! none of the three is a compiler-semantic call graph by itself.

use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::fmt::Write as _;
use std::fs::Metadata;
use std::io;
use std::path::{Path, PathBuf};

fn src_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn required_observation<T>(result: io::Result<T>, action: &str, path: &Path) -> T {
    result.unwrap_or_else(|error| panic!("{action} {}: {error}", path.display()))
}

fn optional_observation<T>(result: io::Result<T>, action: &str, path: &Path) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => panic!("{action} {}: {error}", path.display()),
    }
}

fn metadata_is_reparse_point(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        let _ = metadata;
        false
    }
}

fn reject_link_or_reparse(path: &Path, is_link_or_reparse: bool) {
    assert!(
        !is_link_or_reparse,
        "refusing to follow link or reparse point {}",
        path.display()
    );
}

fn observed_metadata(path: &Path) -> Metadata {
    let metadata = required_observation(
        std::fs::symlink_metadata(path),
        "read symlink metadata for",
        path,
    );
    reject_link_or_reparse(
        path,
        metadata.file_type().is_symlink() || metadata_is_reparse_point(&metadata),
    );
    metadata
}

fn require_regular_non_link(path: &Path) -> Metadata {
    let metadata = observed_metadata(path);
    assert!(
        metadata.is_file(),
        "expected regular file at {}, found another node kind",
        path.display()
    );
    metadata
}

fn read_children_sorted_from<I>(dir: &Path, observed: io::Result<I>) -> Vec<PathBuf>
where
    I: IntoIterator<Item = io::Result<PathBuf>>,
{
    let entries = required_observation(observed, "read directory", dir);
    let mut children: Vec<PathBuf> = entries
        .into_iter()
        .map(|entry| required_observation(entry, "read entry below", dir))
        .collect();
    children.sort();
    children
}

fn read_children_sorted(dir: &Path) -> Vec<PathBuf> {
    let observed =
        std::fs::read_dir(dir).map(|entries| entries.map(|entry| entry.map(|entry| entry.path())));
    read_children_sorted_from(dir, observed)
}

fn enter_bounded_directory(dir: &Path, root_identity: &Path, visited: &mut BTreeSet<PathBuf>) {
    let metadata = observed_metadata(dir);
    assert!(metadata.is_dir(), "expected directory at {}", dir.display());
    let identity = required_observation(
        std::fs::canonicalize(dir),
        "resolve directory identity for",
        dir,
    );
    assert!(
        identity.starts_with(root_identity),
        "directory escaped walk root {}: {}",
        root_identity.display(),
        identity.display()
    );
    assert!(
        visited.insert(identity.clone()),
        "directory identity was visited twice (cycle or alias): {}",
        identity.display()
    );
}

fn source_files_under_bounded(
    dir: &Path,
    root_identity: &Path,
    visited: &mut BTreeSet<PathBuf>,
    out: &mut Vec<PathBuf>,
) {
    enter_bounded_directory(dir, root_identity, visited);
    for path in read_children_sorted(dir) {
        let metadata = observed_metadata(&path);
        if metadata.is_dir() {
            source_files_under_bounded(&path, root_identity, visited, out);
        } else if metadata.is_file() {
            out.push(path);
        }
    }
}

fn source_files_under(root: &Path, out: &mut Vec<PathBuf>) {
    let root_identity = required_observation(
        std::fs::canonicalize(root),
        "resolve source-walk root identity for",
        root,
    );
    let mut visited = BTreeSet::new();
    source_files_under_bounded(root, &root_identity, &mut visited, out);
    out.sort();
}

fn join_normalized_path_segments<'a>(segments: impl IntoIterator<Item = &'a str>) -> String {
    segments.into_iter().collect::<Vec<_>>().join("/")
}

fn normalized_relative_path(path: &Path) -> String {
    join_normalized_path_segments(path.components().map(|component| {
        match component {
            std::path::Component::Normal(segment) => segment
                .to_str()
                .unwrap_or_else(|| panic!("source path is not UTF-8: {}", path.display())),
            other => panic!(
                "source path must be normalized and relative, found {other:?}: {}",
                path.display()
            ),
        }
    }))
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
    source_files_under(&src, &mut files);
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
        let display = normalized_relative_path(
            file.strip_prefix(src.parent().expect("src has a parent"))
                .expect("file under repo"),
        );
        for (number, line) in text.lines().enumerate() {
            // Bidi marks are flagged before ANY exemption. Round 12 caught
            // the file claiming they are "flagged OUTRIGHT" while the prose
            // exemption below silently absorbed one sitting on a `//` line:
            // the matcher named it and the sweep then forgave it. They have
            // no legitimate use in this source — `src/` holds zero — so
            // neither prose nor an allowlist entry may carry one.
            if line.contains('\u{200E}') || line.contains('\u{200F}') {
                result.violations.push(format!(
                    "{display}:{}: [bidi mark] {}",
                    number + 1,
                    line.trim()
                ));
                continue;
            }
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

/// The activation cut's executed-reachability roster: live files where call
/// edges into the lifecycle directory are INTENDED, extended deliberately
/// with each wiring commit. Every row must actually hold at least one edge
/// (anti-vacuity below), so a stale row fails as loudly as an unplanned
/// edge. This is the inversion the frozen retirement contract prescribes:
/// "After activation, the executed Slice 4 reachability cases replace the
/// preactivation census."
const WIRED_PRODUCTION_FILES: &[&str] = &[
    // C5 prep: the stdio bootstrap edges moved from `src/main.rs` into the
    // hoisted dispatcher `src/cli/entry.rs`; the shim main.rs holds none.
    "src/cli/entry.rs",
    // C5 (the exposure flip): the V11 facade re-exports the boundary
    // wrappers — the one public door into the lifecycle directory.
    "src/embed.rs",
    // C5 SEAM-HEALTH anchors: the live health surface names the V11
    // projection it splits into its two halves.
    "src/live_index/health_view.rs",
    "src/daemon.rs",
    "src/gitignore_hygiene.rs",
    "src/live_index/persist.rs",
    "src/live_index/single_file.rs",
    "src/protocol/edit.rs",
    "src/protocol/edit_tools.rs",
    "src/protocol/knowledge_curation.rs",
    "src/protocol/mod.rs",
    "src/protocol/tools.rs",
    "src/server/mod.rs",
    "src/server/serve.rs",
    "src/sidecar/handlers.rs",
    "src/sidecar/mod.rs",
    "src/sidecar/server.rs",
    "src/watcher/mod.rs",
];

/// The frozen publication_roots census (C4): the state structs that retired
/// their bare `index: SharedIndex` / `project_indexes: ..SharedIndex..`
/// fields for the D1 `ProjectRuntimeHandle`. Scoped per struct so function
/// parameters and locals spelled `index: SharedIndex` — which are values in
/// flight, not stored roots — cannot satisfy or violate the claim.
const ROOT_HOLDER_STRUCTS: &[(&str, &str)] = &[
    ("src/daemon.rs", "ProjectInstance"),
    ("src/daemon.rs", "SessionRuntime"),
    ("src/protocol/mod.rs", "SymForgeServer"),
    ("src/server/mod.rs", "ServerRuntime"),
    ("src/sidecar/mod.rs", "SidecarState"),
];

#[test]
fn root_holders_store_no_bare_shared_index() {
    let repo = src_root().parent().expect("src has a parent").to_path_buf();
    let mut hits = Vec::new();
    for (file, name) in ROOT_HOLDER_STRUCTS {
        let text =
            std::fs::read_to_string(repo.join(file)).unwrap_or_else(|_| panic!("read {file}"));
        let header = format!("struct {name} {{");
        let mut in_struct = false;
        let mut found = false;
        for (number, line) in text.lines().enumerate() {
            if !in_struct {
                if line.trim_start().ends_with(&header)
                    && (line.trim_start().starts_with("struct")
                        || line.trim_start().starts_with("pub struct"))
                {
                    in_struct = true;
                    found = true;
                }
                continue;
            }
            // rustfmt closes a top-level struct at column 0.
            if line == "}" {
                in_struct = false;
                continue;
            }
            let field = line.trim_start();
            if field.starts_with("//") {
                continue;
            }
            let stores_bare = field
                .trim_start_matches("pub(crate) ")
                .trim_start_matches("pub ")
                .starts_with("index: SharedIndex")
                || field.contains("HashMap<String, SharedIndex>");
            if stores_bare {
                hits.push(format!("{file}:{}: {name}::{field}", number + 1));
            }
        }
        assert!(found, "root-holder struct {name} not found in {file}");
    }
    assert!(
        hits.is_empty(),
        "bare SharedIndex stored in root-holder state (frozen publication_roots census):\n{}",
        hits.join("\n")
    );
}

#[test]
fn release_push_subject_validation_uses_last_successful_run_head() {
    let repo = src_root().parent().expect("src has a parent").to_path_buf();
    let release = std::fs::read_to_string(repo.join(".github/workflows/release.yml"))
        .expect("read release.yml");
    assert!(
        release.contains("check-push-range \"$LAST\""),
        "release.yml push subject validation must pass the resolved range start to check-push-range"
    );
    for line in release.lines().filter(|line| line.contains("gh run list")) {
        assert!(
            !line.contains("|| true"),
            "release.yml must fail closed when the prior successful Release lookup fails"
        );
    }
    assert!(
        !release.contains(
            "run: python execution/conventional_commits.py check-push-range \"${{ github.event.before }}\" \"$GITHUB_SHA\""
        ),
        "release.yml must not validate push subjects solely from github.event.before"
    );
}

#[test]
fn dark_call_edges_appear_only_in_the_wired_roster() {
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
            // C5 (the mount flip): the dark directory is declared inside the
            // private `internals` wrapper and re-imported at the crate root;
            // `live_index` aliases it so every historical path resolves. Each
            // half of each pair is a declaration edge, not a call edge —
            // exactly the old live_index mount's status.
            ("src/internals.rs", "#[path = \"index_lifecycle/mod.rs\"]"),
            ("src/internals.rs", "pub mod index_lifecycle;"),
            ("src/lib.rs", "pub(crate) use internals::index_lifecycle;"),
            ("src/lib.rs", "pub use internals::index_lifecycle;"),
            (
                "src/live_index/mod.rs",
                "pub(crate) use crate::index_lifecycle;",
            ),
            ("src/live_index/mod.rs", "pub use crate::index_lifecycle;"),
            // Round 6: quote-bearing comment lines lost the prose
            // exemption (a `"` can be a spanning-string tail handing
            // control back to code), so the legitimate quoting comment is
            // decided here instead of silently tolerated.
            (
                "src/lifecycle_identity.rs",
                "//!     darkness as \"`grep -rn index_lifecycle src/` returns no hit outside it\".",
            ),
        ],
    );

    // Partition every matched line: a roster file's edges are the wiring the
    // cut intends; anything else is an unplanned edge and fails exactly as
    // it did before the cut began.
    let mut wired_counts: std::collections::BTreeMap<&str, usize> = WIRED_PRODUCTION_FILES
        .iter()
        .map(|file| (*file, 0usize))
        .collect();
    let mut unplanned = Vec::new();
    for violation in &result.violations {
        match WIRED_PRODUCTION_FILES
            .iter()
            .find(|file| violation.starts_with(&format!("{file}:")))
        {
            Some(file) => *wired_counts.get_mut(*file).expect("roster key") += 1,
            None => unplanned.push(violation.clone()),
        }
    }
    assert!(
        unplanned.is_empty(),
        "call edges into the lifecycle directory OUTSIDE the wired roster:\n{}",
        unplanned.join("\n")
    );
    for (file, count) in &wired_counts {
        assert!(
            *count > 0,
            "roster row `{file}` holds no call edge — a stale row claims \
             wiring that does not exist; remove it deliberately"
        );
    }
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
        (7, 7),
        "the internals mount pair, the two root re-import arms, the two \
         live_index alias arms, and one quote-bearing prose comment: exactly \
         seven allowlisted lines, each seen exactly once; a duplicate mount \
         or a moved/reworded line must update this test deliberately"
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
fn the_server_door_is_declared_once_and_called_only_by_the_shim() {
    // C5 (the keyword flip executed): the module is PUBLIC and WIRED — one
    // declaration in lib.rs, one caller (the binary shim), and the dark
    // directory's wrap-table STRING lines. Anything else naming the door is
    // an unplanned edge, exactly as the pre-flip form of this test held.
    let result = sweep(
        &contains_token("server_api"),
        None,
        Some(&src_root().join("server_api.rs")),
        &[
            ("src/lib.rs", "pub mod server_api;"),
            // The shim: the door's ONE caller.
            (
                "src/main.rs",
                "match symforge::server_api::run(std::env::args_os().collect()) {",
            ),
            (
                "src/main.rs",
                "Ok(symforge::server_api::ServerExit::Success) => std::process::ExitCode::SUCCESS,",
            ),
            (
                "src/main.rs",
                "Ok(symforge::server_api::ServerExit::RefusedToStart) => std::process::ExitCode::from(2),",
            ),
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
                "\"form\": \"cfg feature=server gated pub mod server_api in src/lib.rs, wired to the crate dispatcher\",",
            ),
            (
                "src/index_lifecycle/public_api.rs",
                "\"activation\": \"executed at C5: the pub(crate) keyword flipped behind the already-present server cfg gate, and the census carries the four server_api atoms in server graphs only - the embed-v11 projection excludes this module, so no embed cell may ever grow them\"",
            ),
            // Round 6: the quote-narrowed prose exemption surfaces the one
            // quote-bearing doc comment naming the module.
            (
                "src/index_lifecycle/public_api.rs",
                "/// * `\"keyword-flip\"` — `server_api`: the `pub(crate)` module whose",
            ),
        ],
    );

    assert!(
        result.violations.is_empty(),
        "production references to the server door outside the shim:\n{}",
        result.violations.join("\n")
    );
    // The declaration must be FOUND, and found in its PUBLIC form: a
    // regression back to pub(crate) (or a second declaration) changes the
    // line, drops it from the allowlist, and fails this test — the census
    // would lose the four contract atoms.
    assert!(
        result
            .allowlisted_seen
            .contains(&("src/lib.rs", "pub mod server_api;")),
        "lib.rs no longer declares server_api as pub; the census just lost \
         the four server_api contract atoms"
    );
    // The shim's call edge must be FOUND: a door nobody calls is the
    // reporting-invariant smell the pre-flip form of this test guarded
    // from the other side.
    assert!(
        result
            .allowlisted_seen
            .iter()
            .any(|(file, line)| *file == "src/main.rs" && line.contains("server_api::run")),
        "the binary shim no longer dispatches through server_api::run"
    );
    // The wrap-table string lines must ALL have been seen: an edited atom
    // string falls off the allowlist and fails above, and a silently deleted
    // one fails here.
    let seen: std::collections::BTreeSet<_> = result.allowlisted_seen.iter().collect();
    assert_eq!(
        (result.allowlisted_seen.len(), seen.len()),
        (12, 12),
        "one lib.rs declaration, three shim lines, seven wrap-table/delta \
         string lines, and one quote-bearing doc comment, each seen EXACTLY \
         ONCE; a duplicate or an edit to any of them updates this allowlist \
         deliberately, got: {:?}",
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
    // they have no legitimate use in this source. That check is NOT in
    // this matcher: round 12 moved it into `sweep`, ahead of the
    // allowlist and the prose exemption, because a matcher that merely
    // NAMES a mark still lets the exemption forgive it. Round 13 deleted
    // the copy that was left here, so "outright" is decided in exactly
    // one place.
    // The alias arm (round 5,
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
        // The bidi arm used to sit here. Round 12 moved the decision into
        // `sweep`, ahead of every exemption, which left this copy
        // unreachable — round 13 called that out as dead code still
        // describing itself in the present tense, so it is deleted rather
        // than left to read like the live rule.
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
            // C5 (the exposure flip): the retired raw modules are declared
            // inside the private `internals` wrapper, whose child files
            // would otherwise resolve under src/internals/ — every remount
            // is a same-directory `#[path]`, allowlisted individually so a
            // new or altered mount stays a deliberate change.
            ("src/internals.rs", "#[path = \"analytics/mod.rs\"]"),
            ("src/internals.rs", "#[path = \"capability/mod.rs\"]"),
            ("src/internals.rs", "#[path = \"cli/mod.rs\"]"),
            ("src/internals.rs", "#[path = \"daemon.rs\"]"),
            ("src/internals.rs", "#[path = \"discovery/mod.rs\"]"),
            ("src/internals.rs", "#[path = \"domain/mod.rs\"]"),
            ("src/internals.rs", "#[path = \"edit_safety/mod.rs\"]"),
            ("src/internals.rs", "#[path = \"git.rs\"]"),
            ("src/internals.rs", "#[path = \"gitignore_hygiene.rs\"]"),
            ("src/internals.rs", "#[path = \"hash.rs\"]"),
            ("src/internals.rs", "#[path = \"idempotency.rs\"]"),
            ("src/internals.rs", "#[path = \"index_lifecycle/mod.rs\"]"),
            ("src/internals.rs", "#[path = \"knowledge/mod.rs\"]"),
            ("src/internals.rs", "#[path = \"live_index/mod.rs\"]"),
            ("src/internals.rs", "#[path = \"observability.rs\"]"),
            ("src/internals.rs", "#[path = \"parsing/mod.rs\"]"),
            ("src/internals.rs", "#[path = \"path_shadow.rs\"]"),
            ("src/internals.rs", "#[path = \"paths.rs\"]"),
            ("src/internals.rs", "#[path = \"process_util.rs\"]"),
            ("src/internals.rs", "#[path = \"protocol/mod.rs\"]"),
            ("src/internals.rs", "#[path = \"server/mod.rs\"]"),
            ("src/internals.rs", "#[path = \"sidecar/mod.rs\"]"),
            ("src/internals.rs", "#[path = \"stel/mod.rs\"]"),
            ("src/internals.rs", "#[path = \"stel_core/mod.rs\"]"),
            ("src/internals.rs", "#[path = \"version_registry.rs\"]"),
            ("src/internals.rs", "#[path = \"watcher/mod.rs\"]"),
            ("src/internals.rs", "#[path = \"watcher_state.rs\"]"),
            ("src/internals.rs", "#[path = \"worktree.rs\"]"),
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
        (33, 33),
        "three test-fixture include!(concat!( sites, the 28 internals-wrapper \
         remounts, the claim_provenance mount, and one quoting comment, each \
         seen EXACTLY ONCE as a (file, line) allowlist entry; a duplicate or \
         a new splice site is a deliberate allowlist change, got: {:?}",
        result.allowlisted_seen
    );
}

/// Every Rust source file excluded from one of the two caller sweeps above.
/// The reviewed baseline at the Round-3 repair contains no impl on a type
/// defined outside this set, no outward alias/re-export, and no registration or
/// exported-ABI hook. This narrow seal diagnoses semantic drift inside the dark
/// implementation set; the whole-source seal below also catches an outside
/// alias/macro bridge that preserves the allowlisted mount spelling. Neither is
/// a Rust name resolver or an adversary-resistant security boundary.
const EXCLUDED_RUNTIME_SOURCE_PATHS: &[&str] = &[
    "index_lifecycle/activation.rs",
    "index_lifecycle/adapters.rs",
    "index_lifecycle/authority.rs",
    "index_lifecycle/candidate.rs",
    "index_lifecycle/capacity.rs",
    "index_lifecycle/embedded.rs",
    "index_lifecycle/mod.rs",
    "index_lifecycle/mutation.rs",
    "index_lifecycle/observer.rs",
    "index_lifecycle/physical_root.rs",
    "index_lifecycle/process_runtime.rs",
    "index_lifecycle/public_api.rs",
    "index_lifecycle/query.rs",
    "index_lifecycle/registry.rs",
    "index_lifecycle/runtime.rs",
    "index_lifecycle/snapshot.rs",
    "index_lifecycle/supervisor.rs",
    "index_lifecycle/transition.rs",
    "index_lifecycle/verification.rs",
    "server_api.rs",
];
const EXCLUDED_RUNTIME_SOURCE_DOMAIN_V1: &[u8] = b"symforge-excluded-runtime-source-set-v1\0";
const EXCLUDED_RUNTIME_SOURCE_PIN_V1: (&str, usize, usize) = (
    "707dd7a7dcbf1d5c70a21983ede40b4b0c56c3c43e70ea398a1690da5f08faad",
    20,
    403_999,
);
const FULL_SOURCE_DOMAIN_V1: &[u8] = b"symforge-full-source-set-v1\0";
const FULL_SOURCE_PIN_V1: (&str, usize, usize) = (
    "40e8d9384930ce7dc0c3eae6184596494d5d2db382cc7354f458ebf6865dd241",
    196,
    9_332_480,
);

fn crlf_to_lf(bytes: &[u8]) -> Vec<u8> {
    let mut normalized = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\r' && bytes.get(index + 1) == Some(&b'\n') {
            normalized.push(b'\n');
            index += 2;
        } else {
            normalized.push(bytes[index]);
            index += 1;
        }
    }
    normalized
}

fn normalized_source_records(src: &Path, files: Vec<PathBuf>) -> Vec<(String, Vec<u8>)> {
    let mut records: Vec<(String, Vec<u8>)> = files
        .into_iter()
        .map(|path| {
            let relative = path.strip_prefix(src).unwrap_or_else(|_| {
                panic!(
                    "sealed source escaped src root {}: {}",
                    src.display(),
                    path.display()
                )
            });
            let relative = normalized_relative_path(relative);
            let bytes = required_observation(std::fs::read(&path), "read sealed source", &path);
            (relative, crlf_to_lf(&bytes))
        })
        .collect();
    records.sort_by(|left, right| left.0.cmp(&right.0));
    for pair in records.windows(2) {
        assert_ne!(
            pair[0].0, pair[1].0,
            "two source paths collapsed to one normalized record: {}",
            pair[0].0
        );
    }
    records
}

fn source_set_fingerprint(domain: &[u8], records: &[(String, Vec<u8>)]) -> (String, usize, usize) {
    let mut hash = Sha256::new();
    hash.update(domain);
    hash.update((records.len() as u64).to_le_bytes());
    let mut normalized_bytes = 0usize;
    for (path, content) in records {
        hash.update((path.len() as u64).to_le_bytes());
        hash.update(path.as_bytes());
        hash.update((content.len() as u64).to_le_bytes());
        hash.update(content);
        normalized_bytes += content.len();
    }
    let mut digest = String::with_capacity(64);
    for byte in hash.finalize() {
        write!(&mut digest, "{byte:02x}").expect("write SHA-256 hex");
    }
    (digest, records.len(), normalized_bytes)
}

#[test]
fn excluded_runtime_source_set_matches_reviewed_baseline() {
    let src = src_root();
    let mut files = Vec::new();
    source_files_under(&src.join("index_lifecycle"), &mut files);
    let server_api = src.join("server_api.rs");
    require_regular_non_link(&server_api);
    files.push(server_api);

    let records = normalized_source_records(&src, files);

    let observed_paths: Vec<&str> = records.iter().map(|(path, _)| path.as_str()).collect();
    assert_eq!(
        observed_paths, EXCLUDED_RUNTIME_SOURCE_PATHS,
        "the excluded runtime source set changed; inspect the addition, removal, or rename \
         before updating both the path set and its reviewed semantic baseline"
    );

    let (digest, file_count, normalized_bytes) =
        source_set_fingerprint(EXCLUDED_RUNTIME_SOURCE_DOMAIN_V1, &records);
    assert_eq!(
        (digest.as_str(), file_count, normalized_bytes),
        EXCLUDED_RUNTIME_SOURCE_PIN_V1,
        "an excluded runtime source changed. Direct callers may still be absent while a \
         trait, inherent-method, registration, or re-export bridge creates a semantic edge; \
         review the complete excluded-source diff before updating this pin"
    );
}

/// Freeze every regular in-tree source candidate reviewed for T051. The
/// lexical sweeps explain direct edges and the narrow seal explains dark-side
/// semantic drift; this broader change detector also catches zero-token
/// aliases, macro bridges, and arbitrary Cargo target files outside the dark
/// implementation set. Updating it requires re-reviewing the complete `src/`
/// diff. Generated `OUT_DIR`, proc-macro, dependency, and external-consumer
/// source remain outside this in-repository claim.
#[test]
fn full_source_set_matches_reviewed_darkness_baseline() {
    assert_ne!(
        join_normalized_path_segments(["a", "b.rs"]),
        join_normalized_path_segments([r"a\b.rs"]),
        "a literal backslash filename must not collide with a nested path record"
    );
    let src = src_root();
    let mut files = Vec::new();
    source_files_under(&src, &mut files);
    let records = normalized_source_records(&src, files);
    let (digest, file_count, normalized_bytes) =
        source_set_fingerprint(FULL_SOURCE_DOMAIN_V1, &records);
    assert_eq!(
        (digest.as_str(), file_count, normalized_bytes),
        FULL_SOURCE_PIN_V1,
        "an in-tree source candidate changed. Re-review the complete src diff for a new direct, \
         aliased, macro-generated, trait, inherent-method, registration, or re-export bridge \
         before updating this pin"
    );
}

/// Each CI workflow's whole-file fingerprint, `<fnv1a-64>:<bytes>` over
/// LF-normalized content. Round 14's backstop: the checks below read
/// lines, CI executes YAML scalars, and that seam leaked twice — so no
/// workflow byte may change without a human revisiting the judgement
/// that no gate builds doctests. A change detector, not a security
/// boundary: FNV-1a is not collision-resistant, and whoever edits a
/// workflow can edit this too. What it buys is that the edit is never
/// silent.
const WORKFLOW_FINGERPRINTS: &[(&str, &str)] = &[
    ("ci.yml", "26d8df149f93dc45:14056"),
    ("release.yml", "6e78e0e18ea045c6:111477"),
];

/// The repo's cargo config, verbatim. Pinned rather than parsed: an
/// `[alias]` table here re-points a gate command without touching a
/// workflow line, and every syntax-matching attempt at finding one has
/// lost to the syntax (round 13 broke three spellings of the table
/// header). Update this constant in the same change that edits the file.
const CARGO_CONFIG: &str = "\
# Build artifacts stay beside the checkout, on whatever drive the repo is on
# (see CLAUDE.md, Windows build cache). Not a fixed drive: this said \"on E:\" while
# the checkout lived there, which went stale when it moved.
[build]
target-dir = \"target\"
";

/// Every line of every CI workflow that mentions cargo OR rustdoc,
/// normalized (trimmed, runs of ASCII space and tab collapsed), WITH
/// THE NUMBER OF TIMES it must occur. This is the pin: a human read
/// each one and judged that it cannot build doctests. Grouped by why,
/// so the judgement is auditable rather than asserted.
///
/// Round 12: the counts are per line because a (total, distinct) pair
/// is only a bijection when the two are EQUAL. Four of these lines
/// legitimately occur twice, so at (30, 26) a compensated edit — delete
/// one copy of a gate, add a duplicate of some other allowlisted line —
/// held both numbers while removing `cargo test --all-targets` from PR
/// CI entirely. The multiset is what was always meant; now it is what
/// is checked.
///
/// Round 15: this block belongs HERE, immediately above the constant it
/// describes. Rounds 13 and 14 each inserted a new const into this slot
/// and left the doc attached to the newcomer, so rustdoc documented a
/// two-entry fingerprint list as the per-line cargo allowlist. A blank
/// line does NOT end a `///` run — only an intervening item does — so
/// the separation that reads like a fix is inert. Put new constants
/// BELOW this one.
const CARGO_LINES: &[(&str, usize)] = &[
    // Prose and configuration — never a command.
    (
        "# `cargo check` used to run here as its own full dev-profile pass. Clippy",
        1,
    ),
    (
        "# here silently loses when rust-toolchain.toml is bumped: cargo follows",
        1,
    ),
    (
        "# lints, and more targets (`cargo check` covers lib+bins only). Keeping",
        1,
    ),
    (
        "# types. `cargo tree -d` cannot distinguish this (it lists any",
        1,
    ),
    ("- name: Run cargo check", 1),
    (
        "CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}",
        1,
    ),
    (
        "SYMFORGE_LIFECYCLE_CARGO_EXECUTABLE: ${{ steps.trusted-tools.outputs.cargo }}",
        1,
    ),
    ("cargo-publish:", 1),
    ("environment: cargo-publish", 1),
    // Commands that invoke no test harness at all.
    ("[\"cargo\", \"metadata\", \"--format-version\", \"1\"],", 1),
    (
        "echo \"cargo=$(resolve_tool cargo)\" >> \"$GITHUB_OUTPUT\"",
        1,
    ),
    ("python execution/release_ops.py publish-cargo", 1),
    ("run: cargo build --no-default-features --features embed", 2),
    (
        "run: cargo build --no-default-features --features embed --target x86_64-unknown-linux-musl",
        1,
    ),
    ("run: cargo build --release", 2),
    (
        "run: cargo build --release --target ${{ matrix.target }}",
        1,
    ),
    ("run: cargo check", 1),
    ("run: cargo clippy --all-targets -- -D warnings", 1),
    (
        "run: cargo clippy --no-default-features --features embed,__test-internals --lib -- -D warnings",
        1,
    ),
    (
        "run: cargo clippy --no-default-features --features embed,__test-internals --target x86_64-unknown-linux-musl --lib -- -D warnings",
        1,
    ),
    ("run: cargo fmt --check", 1),
    // The seven test gates, five distinct — two run in both workflows.
    // Each carries a doctest-excluding target selector before its bare
    // `--`, which is why the doctest lane stays shut; drop one and the
    // line no longer matches this pin, and drop one COPY and its count
    // no longer matches either.
    // C7: the suite selects `--lib --bins --tests` — explicit target
    // selection SUPPRESSES the doctest lane (doctests build only under
    // `--doc` or a bare no-selection `cargo test`), and the criterion
    // bench target rejects libtest's `--test-threads`.
    (
        "run: cargo test --lib --bins --tests -- --test-threads=1",
        2,
    ),
    // C7: `cargo bench` never builds doctests; criterion's `--test` mode
    // runs one pass per campaign as the bench smoke gate.
    (
        "run: cargo bench --bench observed_refresh_gate_v1 -- --test",
        2,
    ),
    (
        "run: cargo test --no-default-features --features embed --lib -- --test-threads=1",
        2,
    ),
    (
        "run: cargo test --release --test coupling_calibration calibrate_current_repo_smoke -- --ignored --test-threads=1 --nocapture",
        1,
    ),
    (
        "run: cargo test --release --test live_index_integration test_load_perf_1000_files -- --ignored --test-threads=1",
        1,
    ),
    ("run: cargo test --test serve_port -- --test-threads=1", 1),
];

/// Exact ignore rules that justify the two source-tree skips below. This is a
/// readable basis, not a Gitignore parser. The whole-file fingerprint catches
/// a later negation or other semantic drift elsewhere in the file.
const CARGO_CONFIG_SKIP_GITIGNORE_LINES: &[&str] = &["/target", "node_modules/"];
const GITIGNORE_FINGERPRINT: &str = "b5011af9576da616:1186";
const PRODUCTION_TARGET_TOPOLOGY: &[(&str, &str)] =
    &[("lib", "src/lib.rs"), ("bin:symforge", "src/main.rs")];

fn production_target_topology(manifest: &str) -> Vec<(String, String)> {
    let document = manifest
        .parse::<toml_edit::DocumentMut>()
        .unwrap_or_else(|_| panic!("parse root Cargo.toml for production-target topology"));
    let library = document
        .get("lib")
        .and_then(toml_edit::Item::as_table)
        .unwrap_or_else(|| panic!("root Cargo.toml must keep an explicit [lib] table"));
    let library_path = library
        .get("path")
        .and_then(toml_edit::Item::as_str)
        .unwrap_or_else(|| panic!("root Cargo.toml [lib] must keep an explicit string path"));
    let mut targets = vec![("lib".to_string(), library_path.to_string())];

    let binaries = document
        .get("bin")
        .and_then(toml_edit::Item::as_array_of_tables)
        .unwrap_or_else(|| panic!("root Cargo.toml must keep explicit [[bin]] tables"));
    for binary in binaries {
        let name = binary
            .get("name")
            .and_then(toml_edit::Item::as_str)
            .unwrap_or_else(|| panic!("each root [[bin]] must keep an explicit string name"));
        let path = binary
            .get("path")
            .and_then(toml_edit::Item::as_str)
            .unwrap_or_else(|| panic!("each root [[bin]] must keep an explicit string path"));
        targets.push((format!("bin:{name}"), path.to_string()));
    }
    targets
}

fn require_production_targets_beneath_src(repo: &Path, manifest: &str) {
    let targets = production_target_topology(manifest);
    let expected: Vec<(String, String)> = PRODUCTION_TARGET_TOPOLOGY
        .iter()
        .map(|(kind, path)| ((*kind).to_string(), (*path).to_string()))
        .collect();
    assert!(
        targets == expected,
        "root Cargo.toml production lib/bin target topology changed; inspect every target before \
         updating PRODUCTION_TARGET_TOPOLOGY"
    );

    let src = repo.join("src");
    let src_identity = required_observation(
        std::fs::canonicalize(&src),
        "resolve production source root identity for",
        &src,
    );
    for (_, relative) in targets {
        let target = repo.join(relative);
        require_regular_non_link(&target);
        let identity = required_observation(
            std::fs::canonicalize(&target),
            "resolve production target identity for",
            &target,
        );
        assert!(
            identity.starts_with(&src_identity),
            "production target escaped source sweep root {}: {}",
            src_identity.display(),
            identity.display()
        );
    }
}

#[derive(Debug)]
struct CargoConfigObservation {
    logical_path: PathBuf,
    identity: PathBuf,
}

fn ascii_component_candidate(path: &Path, expected: &str) -> bool {
    path.file_name()
        .and_then(OsStr::to_str)
        .is_some_and(|actual| actual.eq_ignore_ascii_case(expected))
}

fn filesystem_aliases_component(path: &Path, parent: &Path, expected: &str) -> bool {
    if !ascii_component_candidate(path, expected) {
        return false;
    }
    let observed_identity = required_observation(
        std::fs::canonicalize(path),
        "resolve observed directory identity for",
        path,
    );
    let expected_path = parent.join(expected);
    let Some(expected_identity) = optional_observation(
        std::fs::canonicalize(&expected_path),
        "resolve expected directory identity for",
        &expected_path,
    ) else {
        return false;
    };
    observed_identity == expected_identity
}

fn git_ignore_decision(code: Option<i32>, path: &Path) -> bool {
    match code {
        Some(0) => true,
        Some(1) => false,
        other => panic!(
            "`git check-ignore` failed for {} with status {other:?}",
            path.display()
        ),
    }
}

fn git_stdin_path_record(path: &Path) -> Vec<u8> {
    #[cfg(unix)]
    let path_bytes = {
        use std::os::unix::ffi::OsStrExt;
        path.as_os_str().as_bytes().to_vec()
    };
    #[cfg(windows)]
    let path_bytes = {
        let mut encoded = Vec::new();
        for component in path.components() {
            let std::path::Component::Normal(component) = component else {
                panic!(
                    "Git stdin path must be a normalized relative path: {}",
                    path.display()
                );
            };
            if !encoded.is_empty() {
                encoded.push(b'/');
            }
            let component = component.to_str().unwrap_or_else(|| {
                panic!(
                    "Git for Windows cannot observe a non-Unicode path component below {}",
                    path.display()
                )
            });
            encoded.extend_from_slice(component.as_bytes());
        }
        encoded
    };
    #[cfg(not(any(unix, windows)))]
    let path_bytes = path
        .to_str()
        .unwrap_or_else(|| panic!("Git cannot observe non-Unicode path {}", path.display()))
        .as_bytes()
        .to_vec();
    assert!(!path_bytes.is_empty(), "Git stdin path cannot be empty");
    // `check-ignore --stdin` treats records as pathnames except that a leading
    // `:` still activates pathspec magic. A lexical `./` keeps the same
    // repository-relative pathname while making that first byte unambiguous.
    let mut record = b"./".to_vec();
    record.extend_from_slice(&path_bytes);
    record.push(b'\0');
    record
}

fn git_check_ignore(repo: &Path, relative: &Path) -> bool {
    let mut child = symforge::process_util::hidden_command("git")
        .args(["check-ignore", "--no-index", "--stdin", "-z"])
        .current_dir(repo)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap_or_else(|error| {
            panic!(
                "run `git check-ignore --stdin` for {}: {error}",
                relative.display()
            )
        });
    {
        let mut stdin = child
            .stdin
            .take()
            .expect("piped git check-ignore stdin must exist");
        std::io::Write::write_all(&mut stdin, &git_stdin_path_record(relative)).unwrap_or_else(
            |error| {
                panic!(
                    "write `git check-ignore --stdin` path {}: {error}",
                    relative.display()
                )
            },
        );
    }
    let status = child.wait().unwrap_or_else(|error| {
        panic!(
            "wait for `git check-ignore --stdin` on {}: {error}",
            relative.display()
        )
    });
    git_ignore_decision(status.code(), relative)
}

fn require_no_tracked_output(path: &Path, stdout: &[u8]) {
    let records = stdout
        .split(|byte| *byte == b'\0')
        .filter(|record| !record.is_empty())
        .count();
    assert_eq!(
        records,
        0,
        "{records} tracked path(s) exist below ignored directory {}; an \
         ignored-after-tracking config could hide there",
        path.display()
    );
}

fn literal_icase_pathspec(path: &Path) -> OsString {
    let mut pathspec = OsString::from(":(literal,icase)");
    pathspec.push(path.as_os_str());
    pathspec
}

fn should_skip_ignored_dir(repo: &Path, path: &Path) -> bool {
    let relative = path.strip_prefix(repo).unwrap_or_else(|_| {
        panic!(
            "skip candidate escaped repository {}: {}",
            repo.display(),
            path.display()
        )
    });
    assert!(
        !relative.as_os_str().is_empty(),
        "repository root cannot be a skip candidate"
    );
    if !git_check_ignore(repo, relative) {
        return false;
    }

    let tracked = symforge::process_util::hidden_command("git")
        .args(["ls-files", "-z", "--"])
        .arg(literal_icase_pathspec(relative))
        .current_dir(repo)
        .output()
        .unwrap_or_else(|error| panic!("run `git ls-files` below {}: {error}", relative.display()));
    assert!(
        tracked.status.success(),
        "`git ls-files` failed below {} with status {:?}",
        relative.display(),
        tracked.status.code()
    );
    require_no_tracked_output(relative, &tracked.stdout);
    true
}

fn optional_regular_config(path: &Path) -> Option<CargoConfigObservation> {
    let metadata = optional_observation(
        std::fs::symlink_metadata(path),
        "read optional Cargo-config metadata for",
        path,
    )?;
    reject_link_or_reparse(
        path,
        metadata.file_type().is_symlink() || metadata_is_reparse_point(&metadata),
    );
    assert!(
        metadata.is_file(),
        "Cargo config candidate is not a regular file: {}",
        path.display()
    );
    let identity = required_observation(
        std::fs::canonicalize(path),
        "resolve Cargo-config identity for",
        path,
    );
    Some(CargoConfigObservation {
        logical_path: path.to_path_buf(),
        identity,
    })
}

fn cargo_configs_under(
    dir: &Path,
    repo: &Path,
    repo_identity: &Path,
    visited: &mut BTreeSet<PathBuf>,
    found: &mut Vec<CargoConfigObservation>,
) {
    enter_bounded_directory(dir, repo_identity, visited);
    for path in read_children_sorted(dir) {
        let metadata = observed_metadata(&path);
        if !metadata.is_dir() {
            continue;
        }

        if filesystem_aliases_component(&path, dir, ".git") {
            continue;
        }
        let skip_candidate = ascii_component_candidate(&path, "node_modules")
            || (dir == repo && ascii_component_candidate(&path, "target"));
        if skip_candidate && should_skip_ignored_dir(repo, &path) {
            continue;
        }

        if filesystem_aliases_component(&path, dir, ".cargo") {
            let logical_cargo = dir.join(".cargo");
            for candidate in ["config.toml", "config"] {
                if let Some(config) = optional_regular_config(&logical_cargo.join(candidate)) {
                    found.push(config);
                }
            }
            continue;
        }
        cargo_configs_under(&path, repo, repo_identity, visited, found);
    }
}

fn partition_root_config(
    mut configs: Vec<CargoConfigObservation>,
    root_identity: &Path,
) -> Vec<PathBuf> {
    configs.sort_by(|left, right| left.logical_path.cmp(&right.logical_path));
    let root_count = configs
        .iter()
        .filter(|config| config.identity == root_identity)
        .count();
    assert_eq!(
        root_count, 1,
        "the pinned root Cargo config was discovered {root_count} times; exactly one \
         physical observation is required before the stray check can be trusted"
    );
    configs
        .into_iter()
        .filter(|config| config.identity != root_identity)
        .map(|config| config.logical_path)
        .collect()
}

#[test]
fn walk_observation_seams_fail_closed_and_controls_pass() {
    let synthetic = Path::new("synthetic-walk-node");
    let sorted = read_children_sorted_from(
        synthetic,
        Ok(vec![Ok(PathBuf::from("z")), Ok(PathBuf::from("a"))]),
    );
    assert_eq!(sorted, [PathBuf::from("a"), PathBuf::from("z")]);
    let extensionless_root = tempfile::tempdir().expect("create extensionless source-walk control");
    let extensionless = extensionless_root.path().join("production-target");
    std::fs::write(&extensionless, b"fn main() {}\n")
        .expect("write extensionless source-walk control");
    let mut extensionless_scan = Vec::new();
    source_files_under(extensionless_root.path(), &mut extensionless_scan);
    assert_eq!(
        extensionless_scan,
        [extensionless],
        "Cargo accepts extensionless explicit target paths, so every regular src file must be swept"
    );
    assert!(
        std::panic::catch_unwind(|| {
            let read_error: io::Result<Vec<io::Result<PathBuf>>> =
                Err(io::Error::new(io::ErrorKind::PermissionDenied, "read"));
            read_children_sorted_from(synthetic, read_error)
        })
        .is_err(),
        "a read-directory error must not become an empty successful walk"
    );
    assert!(
        std::panic::catch_unwind(|| {
            let entry_error = Ok(vec![Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "entry",
            ))]);
            read_children_sorted_from(synthetic, entry_error)
        })
        .is_err(),
        "a directory-entry error must not be flattened away"
    );
    assert_eq!(required_observation(Ok(7_u8), "observe", synthetic), 7);
    assert!(
        std::panic::catch_unwind(|| {
            required_observation::<u8>(
                Err(io::Error::new(io::ErrorKind::PermissionDenied, "metadata")),
                "observe",
                synthetic,
            )
        })
        .is_err(),
        "a required metadata or identity error must fail closed"
    );
    assert_eq!(
        optional_observation(Ok(9_u8), "observe optional", synthetic),
        Some(9)
    );
    assert_eq!(
        optional_observation::<u8>(
            Err(io::Error::new(io::ErrorKind::NotFound, "missing")),
            "observe optional",
            synthetic,
        ),
        None,
        "NotFound is the sole accepted optional-config absence"
    );
    assert!(
        std::panic::catch_unwind(|| {
            optional_observation::<u8>(
                Err(io::Error::new(io::ErrorKind::PermissionDenied, "optional")),
                "observe optional",
                synthetic,
            )
        })
        .is_err(),
        "an optional-config error other than NotFound must fail closed"
    );
    reject_link_or_reparse(synthetic, false);
    assert!(
        std::panic::catch_unwind(|| reject_link_or_reparse(synthetic, true)).is_err(),
        "a link or reparse point must never be followed"
    );

    let this_source = Path::new(file!());
    assert!(require_regular_non_link(this_source).is_file());
    assert!(
        std::panic::catch_unwind(|| require_regular_non_link(Path::new("tests"))).is_err(),
        "a directory cannot satisfy a regular-file pin"
    );
    let missing = Path::new("tests/__slice3_round3_missing_metadata_probe");
    assert!(
        !missing
            .try_exists()
            .expect("observe missing-path probe collision"),
        "missing-path probe collided with the tree"
    );
    assert!(
        std::panic::catch_unwind(|| observed_metadata(missing)).is_err(),
        "a required metadata error must not become an absent node"
    );

    let tests = Path::new("tests");
    let tests_identity = std::fs::canonicalize(tests).expect("resolve tests directory");
    let mut visited = BTreeSet::new();
    enter_bounded_directory(tests, &tests_identity, &mut visited);
    assert_eq!(visited.len(), 1, "accepting directory enters once");
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            enter_bounded_directory(tests, &tests_identity, &mut visited)
        }))
        .is_err(),
        "a repeated canonical directory identity must fail as an alias/cycle"
    );
    let src = Path::new("src");
    assert!(
        std::panic::catch_unwind(|| {
            let mut outside_visited = BTreeSet::new();
            enter_bounded_directory(src, &tests_identity, &mut outside_visited)
        })
        .is_err(),
        "a canonical directory outside the walk root must fail confinement"
    );
}

#[test]
fn cargo_walk_policy_controls() {
    let path = Path::new("execution/node_modules");
    assert_eq!(
        literal_icase_pathspec(Path::new("NODE_MODULES")),
        OsString::from(":(literal,icase)NODE_MODULES")
    );
    let repo = src_root().parent().expect("src has a parent").to_path_buf();
    assert!(git_check_ignore(&repo, Path::new("target")));
    assert!(
        !git_check_ignore(&repo, Path::new(":(top)target")),
        "git-check-ignore stdin must treat pathspec magic bytes as a literal pathname"
    );
    let manifest_path = repo.join("Cargo.toml");
    let manifest = std::fs::read_to_string(&manifest_path)
        .expect("read manifest for production-target topology controls");
    require_production_targets_beneath_src(&repo, &manifest);
    let extra_target = format!(
        "{manifest}\n[[bin]]\nname = \"outside-source-sweep\"\npath = \
         \"execution/outside-source-sweep\"\n"
    );
    assert!(
        std::panic::catch_unwind(|| {
            require_production_targets_beneath_src(&repo, &extra_target)
        })
        .is_err(),
        "an added explicit production target must not escape topology review"
    );
    assert!(git_ignore_decision(Some(0), path));
    assert!(!git_ignore_decision(Some(1), path));
    assert!(
        std::panic::catch_unwind(|| git_ignore_decision(Some(2), path)).is_err(),
        "a git check-ignore execution error must fail closed"
    );
    assert!(
        std::panic::catch_unwind(|| git_ignore_decision(None, path)).is_err(),
        "a signalled git check-ignore process must fail closed"
    );
    require_no_tracked_output(path, b"");
    assert!(
        std::panic::catch_unwind(|| require_no_tracked_output(path, b"opaque-\xff\0")).is_err(),
        "raw non-UTF-8 tracked output must be detected without decoding"
    );

    let root_identity = Path::new("root-config-identity");
    assert!(
        std::panic::catch_unwind(|| partition_root_config(Vec::new(), root_identity)).is_err(),
        "zero discovered root configs must fail anti-vacuity"
    );
    let root = CargoConfigObservation {
        logical_path: PathBuf::from(".cargo/config.toml"),
        identity: root_identity.to_path_buf(),
    };
    assert!(partition_root_config(vec![root], root_identity).is_empty());
}

#[test]
fn no_gate_builds_doctests() {
    // Round 7: rustdoc extracts fenced doc-comment text into doctest
    // crates that a bare `cargo test` (or `--doc`) builds and RUNS — an
    // executing edge the prose exemption above would tolerate. The
    // inert-comment rule is therefore bounded by the gates never opening
    // the doctest lane, and this test OBSERVES that bound instead of
    // asserting it from memory.
    //
    // Rounds 8, 9, 10 and 11 each falsified a scan that tried to READ the
    // command out of the workflow and judge it. The graveyard is worth
    // keeping, because every entry is the same mistake: a `.yml`-only
    // filter and sibling-token masking (8); quoted scalars, plain
    // multi-line scalars, flow mappings and the `cargo t` alias (9);
    // `+toolchain` and space-separated global option values, which broke
    // the subcommand finder (10); and finally (11) `cargo rustdoc --
    // --test`, which builds and runs doctests while naming neither
    // `test` nor `--doc` in a position the walk judged — plus
    // `cargo te"st" --doc`, where the scan's own quote-erasure SPLIT a
    // word the shell JOINS, and `X=$(cargo test --doc)`, where shell
    // grouping hid the token. Four rounds, one lesson, the same one
    // rounds 1–3 taught about Rust: a scan that must MODEL a syntax to
    // find the thing loses to that syntax. The shell's word rules are
    // not this test's to reimplement.
    //
    // So it stopped reading commands. Every line of every workflow that
    // mentions cargo OR rustdoc — case-insensitively, so `CARGO_*`
    // counts — must appear VERBATIM in `CARGO_LINES` above, normalized
    // by trimming and collapsing runs of ASCII space and tab, which is
    // what bash's default IFS splits on within a line. A human judged
    // each of those lines doctest-free; anything else, in any spelling,
    // quoting, grouping or subcommand, fails and forces that judgement
    // to be made again. Round 11 claimed "there is no word model left
    // to be wrong about" here; round 13 falsified it (normalization is
    // itself a word model, and it disagreed with bash over U+00A0), and
    // round 14 showed the deeper seam — the unit compared is a LINE
    // while the unit executed is a YAML scalar. The line checks are
    // kept for the auditable judgement they record; the whole-file
    // fingerprints below are what make a change impossible to miss.
    //
    // KNOWN UNDER-COVERAGE RESIDUALS — what has been probed, NOT a proof of
    // exhaustiveness (round 9 wrote "the only two" and round 10 produced
    // a third the same day; round 11 produced a fourth; round 13 broke
    // the check that was meant to retire one). The current two behavioural
    // coverage bounds are:
    //   * A gate whose EFFECT lives outside these files — a script, make
    //     target, or composite action. Note this is about where the
    //     BEHAVIOUR lives, not whether the line says `cargo`: the
    //     allowlisted `python execution/release_ops.py publish-cargo`
    //     names cargo and is pinned, yet what that script runs is not.
    //     Round 15 corrected an earlier wording here that conditioned
    //     the residual on the line naming neither `cargo` nor `rustdoc`,
    //     which described a narrower set than the header's.
    //   * A cargo config OUTSIDE the repo — `~/.cargo/config.toml`,
    //     `$CARGO_HOME`, or an ancestor of the checkout — or below an
    //     existing `.cargo` directory, which matters only when that outer
    //     `.cargo` is itself the CWD. No pinned CI gate does that.
    // The header separately records ignored-tree OVER-FLAGS: they create
    // safe friction rather than a false green, so they are not a third gap.
    // Two former residuals are NOT on this list because they are now
    // caught, both proven by mutation: every normally add-visible config
    // relevant to an in-repo, non-`.cargo` working directory is pinned
    // below, and a gate disabled with `if: false` changes no cargo line
    // but does change the file, which the fingerprints see.
    // An earlier version of this comment claimed THREE residuals
    // "matching the header" while the header listed two and retired the
    // third — the contradiction was the tell.
    let repo = src_root().parent().expect("src has a parent").to_path_buf();
    let workflows = repo.join(".github").join("workflows");
    // ROUND 14, THE BACKSTOP. Everything below reads LINES; what CI
    // executes is a YAML scalar, and round 14 walked through that gap
    // twice — a pinned line extended by a continuation that is itself a
    // pinned line (`cargo test --all-targets -- --test-threads=1` +
    // `python execution/release_ops.py publish-cargo`, which libtest
    // swallows as extra filters, exit 0), and a gate RELOCATED from
    // ci.yml to release.yml, which the flat cross-file multiset cannot
    // see. Both leave every line pinned and every count matching.
    // Patching the line walk a fourth time would just move the seam, so
    // the whole file is fingerprinted: the checks below still record
    // WHY each cargo line is doctest-free, and this one guarantees no
    // workflow byte changed without a human revisiting that judgement.
    // This is a CHANGE DETECTOR, not a security boundary — FNV-1a is
    // not collision-resistant, and anyone editing a workflow can edit
    // this constant too. The property it buys is that the edit cannot
    // be SILENT.
    fn fingerprint(text: &str) -> String {
        let normalized = text.replace("\r\n", "\n");
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in normalized.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        format!("{hash:016x}:{}", normalized.len())
    }
    let manifest_path = repo.join("Cargo.toml");
    require_regular_non_link(&manifest_path);
    let manifest = std::fs::read_to_string(&manifest_path)
        .expect("read root Cargo.toml for production-target topology");
    require_production_targets_beneath_src(&repo, &manifest);
    let mut seen: Vec<String> = Vec::new();
    let mut offenders = Vec::new();
    let mut files = 0usize;
    let mut fingerprints: Vec<(String, String)> = Vec::new();
    for entry in std::fs::read_dir(&workflows).expect("read workflows dir") {
        let path = entry.expect("workflow entry").path();
        if path.extension().is_none_or(|e| e != "yml" && e != "yaml") {
            continue;
        }
        files += 1;
        let text = std::fs::read_to_string(&path).expect("read workflow");
        fingerprints.push((
            path.file_name()
                .expect("file name")
                .to_string_lossy()
                .into_owned(),
            fingerprint(&text),
        ));
        for (number, line) in text.lines().enumerate() {
            // Round 13: `rustdoc` selects too. The file already named it as
            // an equally sufficient spelling of the doctest lane — and then
            // applied that knowledge only to the ALLOWLIST, never to the
            // workflow text, so a first-class `run: rustdoc --test src/lib.rs`
            // step (no cargo anywhere) walked past the filter entirely.
            let lowered = line.to_ascii_lowercase();
            if !lowered.contains("cargo") && !lowered.contains("rustdoc") {
                continue;
            }
            // Split on ASCII space and tab ONLY, which is what bash's
            // default IFS splits on. Round 13: `split_whitespace` uses the
            // Unicode White_Space property, so a U+00A0 between `--` and
            // `--test-threads=1` normalized to exactly the pinned gate
            // while bash saw a single glued word — the pin could not tell
            // the pinned command from one that is not it.
            let normalized = line
                .split([' ', '\t'])
                .filter(|t| !t.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            if CARGO_LINES.iter().any(|(l, _)| *l == normalized) {
                seen.push(normalized);
                continue;
            }
            offenders.push(format!(
                "{}:{}: {normalized}",
                path.file_name().expect("file name").to_string_lossy(),
                number + 1
            ));
        }
    }
    assert!(
        offenders.is_empty(),
        "CI workflow lines mentioning cargo or rustdoc that this test has \
         never seen. Each one must be read and added to CARGO_LINES with the \
         group that says why it cannot build doctests — a doctest is an \
         executing edge the prose exemption in this file's header \
         tolerates:\n{}",
        offenders.join("\n")
    );
    // `--doc` and `rustdoc` are two spellings that open the lane
    // without a `cargo test` in sight, so they are ALSO named directly.
    // Not the only two — round 14 pointed out `cargo t` opens it as
    // well, and the round-13 `[alias]` work proved an aliased `fmt`
    // does. This is a cheap second reading over the allowlist, so a
    // careless ADDITION trips on the obvious spellings; the allowlist
    // itself, and the fingerprints below, are what actually bind.
    let named: Vec<&(&str, usize)> = CARGO_LINES
        .iter()
        .filter(|(l, _)| l.contains("--doc") || l.contains("rustdoc"))
        .collect();
    assert!(
        named.is_empty(),
        "an allowlisted line names the doctest lane directly — `cargo rustdoc \
         -- --test` runs doctests and fails the step on failure, exactly like \
         `--doc`:\n{named:?}"
    );
    // The repo's cargo config is PINNED VERBATIM, not searched for an
    // `[alias]` table. Round 12 added that search; round 13 defeated it
    // three ways in one afternoon — TOML accepts `[ alias ]` and
    // `["alias"]` as the same table header, a root-level `alias.fmt =
    // [...]` dotted key declares it with no header at all, and cargo
    // still honours the legacy extensionless `.cargo/config`, which the
    // search never opened. Weaponized, `[ alias ]` + `fmt = ["test",
    // "--doc", "--", "--skip"]` turned the allowlisted `run: cargo fmt
    // --check` into a full doctest run that exited 0 with both workflow
    // files byte-unchanged. (Aliases cannot shadow BUILT-IN subcommands,
    // so `cargo test` is not re-pointable — `fmt` and `clippy` are
    // external subcommands and are.) Matching TOML syntax is the same
    // mistake as matching YAML or shell syntax, one file format later,
    // so it is not matched: the whole file is pinned, and the legacy
    // path must not exist. STATED RESIDUAL: a config OUTSIDE the repo —
    // `~/.cargo/config.toml`, `$CARGO_HOME`, or an ancestor directory of
    // the checkout. CI runners have none, and a `CARGO_ALIAS_*` env var
    // in a workflow would carry `cargo` in its name and be caught above.
    let cargo_dir = repo.join(".cargo");
    let cargo_dir_metadata = observed_metadata(&cargo_dir);
    assert!(
        cargo_dir_metadata.is_dir(),
        "expected Cargo config directory at {}",
        cargo_dir.display()
    );
    let cargo_config = cargo_dir.join("config.toml");
    require_regular_non_link(&cargo_config);
    let config = std::fs::read_to_string(&cargo_config)
        .expect("read .cargo/config.toml — the pin below cannot vouch for a file it did not read");
    assert!(
        config.replace("\r\n", "\n") == CARGO_CONFIG,
        "`.cargo/config.toml` differs from its pin. Any change here can \
         re-point an allowlisted gate line at a doctest-running command \
         without touching a workflow — read the diff and update this pin \
         deliberately"
    );
    assert!(
        !repo
            .join(".cargo")
            .join("config")
            .try_exists()
            .expect("observe whether legacy .cargo/config exists"),
        "`.cargo/config` (the legacy extensionless path) exists. Cargo reads \
         it exactly like config.toml, so it can carry an [alias] table this \
         pin does not cover — fold it into config.toml or pin it here"
    );
    let gitignore_path = repo.join(".gitignore");
    require_regular_non_link(&gitignore_path);
    let gitignore = std::fs::read_to_string(&gitignore_path)
        .expect("read .gitignore — the directory-skip pin cannot vouch for unread rules");
    assert_eq!(
        fingerprint(&gitignore),
        GITIGNORE_FINGERPRINT,
        "`.gitignore` changed. Its `/target` and `node_modules/` rules justify \
         candidate directories for omission, while `git check-ignore` decides \
         whether each concrete directory is actually skippable. Re-audit both \
         parts before updating GITIGNORE_FINGERPRINT"
    );
    let observed_skip_lines: Vec<&str> = gitignore
        .lines()
        .filter(|line| CARGO_CONFIG_SKIP_GITIGNORE_LINES.contains(line))
        .collect();
    assert_eq!(
        observed_skip_lines.as_slice(),
        CARGO_CONFIG_SKIP_GITIGNORE_LINES,
        "the exact `.gitignore` rules that nominate root `target` and \
         `node_modules` as skip candidates changed. Git still decides each \
         concrete directory's effective normal-add visibility"
    );
    // Round 14: cargo merges `.cargo/config.toml` from the CWD and every
    // ancestor, so a config one directory DOWN is honoured the moment a
    // step sets `working-directory:`. Pinning the ROOT config alone left
    // that open, and the adjudicator drove it end to end: an alias in
    // `execution/.cargo/config.toml` turned `cargo fmt --check` into a
    // doctest run, an inert `///` line called into the dark directory,
    // and the marker file was written — the darkness guarantee failing
    // with the tripwire reporting all-clear. So the bounded walk must find
    // each `.cargo` directory relevant to an in-repo source working directory,
    // not just the root one. It intentionally does not recurse inside a
    // `.cargo` directory: `.cargo/.cargo/config.toml` matters only if the outer
    // `.cargo` itself becomes the CWD, which the pinned workflows do not do.
    // Round 17: name comparisons are only candidate filters. Cargo follows
    // the filesystem, so an ASCII-case variant counts as `.cargo` only when
    // it resolves to the same directory as the parent's actual `.cargo`
    // lookup. Git, not a home-grown ignore parser, decides whether root
    // `target` or a concrete `node_modules` can be omitted. Before any such
    // omission, raw `git ls-files -z` output under a literal, case-insensitive
    // pathspec must be empty; it is never decoded, so an opaque Unix path cannot
    // turn observation into failure. `git check-ignore` receives one NUL-framed
    // pathname on stdin with a lexical `./`, not a pathspec-capable argv token.
    // Descendant links and Windows reparse points are refused before they can
    // escape the root or create a cycle, and traversal is sorted.
    let repo_identity = required_observation(
        std::fs::canonicalize(&repo),
        "resolve Cargo-config walk root identity for",
        &repo,
    );
    let root_config_identity = required_observation(
        std::fs::canonicalize(&cargo_config),
        "resolve pinned root Cargo-config identity for",
        &cargo_config,
    );
    let mut configs = Vec::new();
    let mut visited = BTreeSet::new();
    cargo_configs_under(&repo, &repo, &repo_identity, &mut visited, &mut configs);
    let stray = partition_root_config(configs, &root_config_identity);
    assert!(
        stray.is_empty(),
        "a cargo config lives somewhere other than the pinned repo root. \
         Cargo merges `.cargo/config.toml` from the working directory and \
         every ancestor, so a descendant config plus a `working-directory:` \
         on any gate re-points that gate — read these and pin them \
         deliberately:\n{stray:#?}"
    );
    // The MULTISET binds, per line. Round 12: (total, distinct) is only a
    // bijection when the two are equal — at (30, 26) a compensated edit
    // deleted `cargo test --all-targets` from PR CI while both numbers
    // held, because the deleted string survived via its twin in the other
    // workflow. Comparing observed counts to the declared ones catches a
    // deletion, a rewording, and a duplicate individually.
    let mut observed: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for line in &seen {
        *observed.entry(line.as_str()).or_default() += 1;
    }
    let declared: std::collections::BTreeMap<&str, usize> =
        CARGO_LINES.iter().map(|(l, n)| (*l, *n)).collect();
    let drifted: Vec<String> = declared
        .iter()
        .filter(|(l, n)| observed.get(**l).copied().unwrap_or(0) != **n)
        .map(|(l, n)| {
            format!(
                "expected {n}x, saw {}x: {l}",
                observed.get(*l).copied().unwrap_or(0)
            )
        })
        .collect();
    assert!(
        drifted.is_empty(),
        "CI workflow cargo lines drifted from their pinned occurrence \
         counts. A gate was removed, reworded, or duplicated — reconcile \
         CARGO_LINES with the workflows deliberately, never by loosening \
         this pin:\n{}",
        drifted.join("\n")
    );
    // The backstop assertion. Any byte of either workflow moving lands
    // here even when every line above still matches — a relocation
    // across files, a continuation extending a pinned command, an `if:`
    // switching a gate off, a `working-directory:` added to one.
    fingerprints.sort();
    let declared_prints: Vec<(String, String)> = WORKFLOW_FINGERPRINTS
        .iter()
        .map(|(f, p)| ((*f).to_string(), (*p).to_string()))
        .collect();
    assert_eq!(
        fingerprints, declared_prints,
        "a CI workflow file changed. Every cargo line may still be \
         allowlisted and every count may still match — a gate moved \
         between files, a continuation extended a pinned command, or a \
         step was disabled — so read the diff, confirm no gate builds \
         doctests, and update WORKFLOW_FINGERPRINTS in the same change"
    );
    let distinct: std::collections::BTreeSet<_> = seen.iter().collect();
    assert_eq!(
        (seen.len(), distinct.len(), files),
        (32, 27, 2),
        "the CI workflows hold thirty-two cargo-mentioning lines, twenty-seven \
         of them distinct, across two workflow files; this walk saw {:?}. A gate \
         added, removed, reworded, or a workflow file added — reconcile \
         CARGO_LINES with the workflows deliberately, never by loosening this \
         pin",
        (seen.len(), distinct.len(), files)
    );
}
