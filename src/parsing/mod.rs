pub mod ast_grep;
pub mod config_extractors;
#[cfg(test)]
mod inline_tests;
pub mod languages;
pub mod xref;

use std::collections::HashMap;
use std::panic;

use tree_sitter::Parser;

use tree_sitter::Node;

use crate::domain::{
    FileClassification, FileOutcome, FileProcessingResult, LanguageId, ParseDiagnostic,
    ReferenceRecord, SymbolRecord,
};
use crate::hash::digest_hex;

type ParseSourceOutput = (
    Vec<SymbolRecord>,
    bool,
    Option<ParseDiagnostic>,
    Vec<ReferenceRecord>,
    HashMap<String, String>,
);

pub fn process_file(
    relative_path: &str,
    bytes: &[u8],
    language: LanguageId,
) -> FileProcessingResult {
    process_file_with_classification(
        relative_path,
        bytes,
        language,
        FileClassification::for_code_path(relative_path),
    )
}

pub fn process_file_with_classification(
    relative_path: &str,
    bytes: &[u8],
    language: LanguageId,
    classification: FileClassification,
) -> FileProcessingResult {
    let byte_len = bytes.len() as u64;
    let content_hash = digest_hex(bytes);

    // Config files use native parsers, not tree-sitter.
    if config_extractors::is_config_language(&language) {
        let result = config_extractors::extractor_for(&language).map(|e| e.extract(bytes));
        let (symbols, outcome, parse_diagnostic) = match result {
            Some(r) => {
                let (outcome, parse_diagnostic) = match r.outcome {
                    config_extractors::ExtractionOutcome::Ok => (FileOutcome::Processed, None),
                    config_extractors::ExtractionOutcome::Partial(diagnostic) => (
                        FileOutcome::PartialParse {
                            warning: diagnostic.summary(),
                        },
                        Some(diagnostic),
                    ),
                    config_extractors::ExtractionOutcome::Failed(diagnostic) => (
                        FileOutcome::Failed {
                            error: diagnostic.summary(),
                        },
                        Some(diagnostic),
                    ),
                };
                (r.symbols, outcome, parse_diagnostic)
            }
            None => (vec![], FileOutcome::Processed, None),
        };
        return FileProcessingResult {
            relative_path: relative_path.to_string(),
            language,
            classification,
            outcome,
            parse_diagnostic,
            symbols,
            byte_len,
            content_hash,
            references: vec![],
            alias_map: HashMap::new(),
        };
    }

    let source = String::from_utf8_lossy(bytes);

    // `.tsx` requires the TSX grammar; `.ts` stays on plain TypeScript. The
    // distinction is carried by the file extension, not the LanguageId.
    let is_tsx = LanguageId::is_tsx_path(relative_path);

    let parse_result = panic::catch_unwind(|| parse_source(&source, &language, is_tsx));

    match parse_result {
        Ok(Ok((symbols, has_error, diagnostic, references, alias_map))) => {
            let outcome = if has_error {
                let warning = diagnostic.as_ref().map(|d| d.summary()).unwrap_or_else(|| {
                    "tree-sitter reported syntax errors in the parse tree".to_string()
                });
                FileOutcome::PartialParse { warning }
            } else {
                FileOutcome::Processed
            };
            FileProcessingResult {
                relative_path: relative_path.to_string(),
                language,
                classification,
                outcome,
                parse_diagnostic: diagnostic,
                symbols,
                byte_len,
                content_hash,
                references,
                alias_map,
            }
        }
        Ok(Err(err)) => FileProcessingResult {
            relative_path: relative_path.to_string(),
            language,
            classification,
            outcome: FileOutcome::Failed {
                error: err.to_string(),
            },
            parse_diagnostic: None,
            symbols: vec![],
            byte_len,
            content_hash,
            references: vec![],
            alias_map: HashMap::new(),
        },
        Err(_panic) => FileProcessingResult {
            relative_path: relative_path.to_string(),
            language,
            classification,
            outcome: FileOutcome::Failed {
                error: "tree-sitter parser panicked during parsing".to_string(),
            },
            parse_diagnostic: None,
            symbols: vec![],
            byte_len,
            content_hash,
            references: vec![],
            alias_map: HashMap::new(),
        },
    }
}

/// Walk the tree-sitter tree and collect info about the deepest useful ERROR or MISSING node.
/// Returns (message, line, column, byte_span) for building a `ParseDiagnostic`.
fn collect_deepest_error_node(root: &Node, source: &str) -> Option<(String, u32, u32, (u32, u32))> {
    let mut cursor = root.walk();
    let mut stack = vec![(*root, 0usize)];
    let mut best: Option<(Node, usize)> = None;

    while let Some((node, depth)) = stack.pop() {
        if node.is_error() || node.is_missing() {
            best = match best {
                Some((current, current_depth))
                    if !error_candidate_is_better(node, depth, current, current_depth) =>
                {
                    Some((current, current_depth))
                }
                _ => Some((node, depth)),
            };
        }
        // Push children in reverse so we visit left-to-right via the stack.
        cursor.reset(node);
        if cursor.goto_first_child() {
            let mut children = vec![cursor.node()];
            while cursor.goto_next_sibling() {
                children.push(cursor.node());
            }
            stack.extend(children.into_iter().rev().map(|child| (child, depth + 1)));
        }
    }

    best.map(|(node, _depth)| {
        let start = node.start_position();
        let snippet_start = node.start_byte().min(source.len());
        // Clamp the 40-byte snippet window down to the nearest UTF-8 char
        // boundary — tree-sitter reports byte offsets, and `snippet_start +
        // 40` can land mid-multibyte-char, which would panic str slicing.
        let snippet_limit = snippet_start + 40;
        let span_end = node.end_byte().min(source.len());
        let mut snippet_end = if span_end > snippet_start {
            span_end.min(snippet_limit)
        } else {
            snippet_limit.min(source.len())
        };
        while snippet_end > snippet_start && !source.is_char_boundary(snippet_end) {
            snippet_end -= 1;
        }
        let snippet = &source[snippet_start..snippet_end];
        let kind = if node.is_missing() {
            format!("missing {}", node.kind())
        } else {
            "error".to_string()
        };
        let message = format!("syntax {kind} near `{}`", snippet.replace('\n', "\\n"));
        (
            message,
            start.row as u32 + 1,    // 1-based line
            start.column as u32 + 1, // 1-based column
            (node.start_byte() as u32, node.end_byte() as u32),
        )
    })
}

fn error_candidate_is_better(
    candidate: Node,
    candidate_depth: usize,
    current: Node,
    current_depth: usize,
) -> bool {
    if candidate_depth != current_depth {
        return candidate_depth > current_depth;
    }

    let candidate_len = candidate.end_byte().saturating_sub(candidate.start_byte());
    let current_len = current.end_byte().saturating_sub(current.start_byte());
    if candidate_len != current_len {
        return candidate_len < current_len;
    }

    if candidate.start_byte() != current.start_byte() {
        return candidate.start_byte() < current.start_byte();
    }

    candidate.is_missing() && !current.is_missing()
}

pub(crate) fn parse_source(
    source: &str,
    language: &LanguageId,
    is_tsx: bool,
) -> Result<ParseSourceOutput, String> {
    let mut parser = Parser::new();

    let ts_language = match language {
        LanguageId::Rust => tree_sitter_rust::LANGUAGE.into(),
        LanguageId::Python => tree_sitter_python::LANGUAGE.into(),
        LanguageId::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
        // `.tsx` needs the JSX-aware TSX grammar; `.ts` keeps the plain
        // TypeScript grammar (which still accepts legacy `<T>expr` casts).
        LanguageId::TypeScript if is_tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
        LanguageId::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        LanguageId::Go => tree_sitter_go::LANGUAGE.into(),
        LanguageId::Java => tree_sitter_java::LANGUAGE.into(),
        LanguageId::C => tree_sitter_c::LANGUAGE.into(),
        LanguageId::Cpp => tree_sitter_cpp::LANGUAGE.into(),
        LanguageId::CSharp => tree_sitter_c_sharp::LANGUAGE.into(),
        LanguageId::Ruby => tree_sitter_ruby::LANGUAGE.into(),
        LanguageId::Php => tree_sitter_php::LANGUAGE_PHP.into(),
        LanguageId::Swift => tree_sitter_swift::LANGUAGE.into(),
        LanguageId::Perl => tree_sitter_perl::LANGUAGE.into(),
        LanguageId::Kotlin => tree_sitter_kotlin_sg::LANGUAGE.into(),
        LanguageId::Dart => tree_sitter_dart::language(),
        LanguageId::Elixir => tree_sitter_elixir::LANGUAGE.into(),
        LanguageId::Json
        | LanguageId::Toml
        | LanguageId::Yaml
        | LanguageId::Markdown
        | LanguageId::Env => unreachable!("config types are handled before parse_source"),
        LanguageId::Html => tree_sitter_html::LANGUAGE.into(),
        LanguageId::Css => tree_sitter_css::LANGUAGE.into(),
        LanguageId::Scss => tree_sitter_scss::language(),
    };

    parser
        .set_language(&ts_language)
        .map_err(|e| format!("failed to set language: {e}"))?;

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| "tree-sitter parse returned None".to_string())?;

    let root = tree.root_node();
    let has_error = root.has_error();
    let symbols = languages::extract_symbols(&root, source, language);
    let (references, alias_map) = xref::extract_references(&root, source, language, is_tsx);

    let diagnostic = if has_error {
        collect_deepest_error_node(&root, source).map(|(message, line, column, span)| {
            ParseDiagnostic {
                parser: "tree-sitter".to_string(),
                message,
                line: Some(line),
                column: Some(column),
                byte_span: Some(span),
                fallback_used: false,
            }
        })
    } else {
        None
    };

    Ok((symbols, has_error, diagnostic, references, alias_map))
}

/// Cache-key discriminant for [`expected_partial_memo`]: which known grammar
/// limitation a cached verdict belongs to.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum ExpectedPartialCheck {
    /// SF-003: TS import-type immediately followed by `[]`.
    TsImportTypeArray,
    /// SF-004: Angular control-flow relational operators in `.html`.
    AngularControlFlow,
}

/// Upper bound on memo entries; at the cap the map is cleared wholesale.
/// Entries are a few machine words each, so the bound is generous; clearing
/// (vs an LRU) keeps the code trivial, and a refill costs one
/// re-classification per still-live file.
const EXPECTED_PARTIAL_MEMO_CAP: usize = 4096;

/// Process-wide memo for the SF-003/SF-004 neutralize-and-reparse verdicts,
/// keyed by (check, content length, content hash).
///
/// The classifiers cost up to two FULL tree-sitter parses per call and sit on
/// render paths invoked repeatedly with identical content: `health_stats`
/// iterates every partial-parse file per `health` call, and the
/// `get_file_context` / `validate_file_syntax` / sidecar envelopes re-classify
/// per render. A file's content is immutable for a given index generation, so
/// a (length, 64-bit content hash) key dedups those re-parses. A wrong-verdict
/// collision requires both equal 64-bit hashes AND equal lengths on different
/// content that is also classified differently — astronomically unlikely for
/// the worst case of a mislabeled health bucket. Stale entries from edited
/// files age out via the wholesale clear at [`EXPECTED_PARTIAL_MEMO_CAP`].
static EXPECTED_PARTIAL_MEMO: std::sync::LazyLock<std::sync::Mutex<ExpectedPartialMemoMap>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(ExpectedPartialMemoMap::new()));

/// Memo key: (which check, content length, 64-bit content hash).
type ExpectedPartialMemoKey = (ExpectedPartialCheck, u64, u64);
/// Verdict map behind [`EXPECTED_PARTIAL_MEMO`].
type ExpectedPartialMemoMap = std::collections::HashMap<ExpectedPartialMemoKey, bool>;

/// Memoize one expected-partial classification: return the cached (check,
/// content) verdict, computing and inserting it on a miss.
///
/// `compute` runs WITHOUT the lock held, so two threads may race to compute
/// the same verdict — harmless (the verdict is deterministic) and preferable
/// to holding a global lock across a tree-sitter parse. A poisoned lock falls
/// back to the inner map: the map holds only derived verdicts, so there is no
/// invariant a panicking writer could have broken.
fn expected_partial_memo(
    check: ExpectedPartialCheck,
    content: &[u8],
    compute: impl FnOnce() -> bool,
) -> bool {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut hasher);
    let key = (check, content.len() as u64, hasher.finish());
    if let Some(&verdict) = EXPECTED_PARTIAL_MEMO
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&key)
    {
        return verdict;
    }
    let verdict = compute();
    let mut memo = EXPECTED_PARTIAL_MEMO
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if memo.len() >= EXPECTED_PARTIAL_MEMO_CAP {
        memo.clear();
    }
    memo.insert(key, verdict);
    verdict
}

/// Matches a TypeScript import-type member (`import('mod').Member`) immediately
/// followed by one or more postfix array suffixes (`[]`), allowing interior
/// whitespace. Capture group 1 is the scalar import-type member alone; the
/// trailing array suffixes are replaced with a single space when this is used
/// as a `${1} ` replacement (token-preserving: never fuses the member with a
/// following identifier fragment).
///
/// This is the construct that `tree-sitter-typescript 0.23.2` mis-parses
/// (SF-003): scalar `import('rxjs').Subscription` parses clean everywhere, but
/// the `[]` array suffix breaks the parse.
static IMPORT_TYPE_ARRAY_SUFFIX_RE: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| {
        regex::Regex::new(
            r#"(import\s*\(\s*['"][^'"]+['"]\s*\)\s*\.\s*[A-Za-z_$][A-Za-z0-9_$]*)(?:\s*\[\s*\])+"#,
        )
        .expect("SF-003 import-type-array regex is a valid pattern")
    });

/// SF-003: recognize a partial parse whose ONLY cause is the known
/// `tree-sitter-typescript 0.23.2` grammar limitation on an import-type
/// immediately followed by a `[]` array suffix (e.g.
/// `import('rxjs').Subscription[]`).
///
/// Soundness — this validates the WHOLE construct, not just the error prefix.
/// The naive "error node text starts with `import(` and the next char is `[`"
/// heuristic is UNSOUND: a genuinely broken file such as
/// `import('rxjs').Subscription[] = [ ; foo bar` produces a byte-identical
/// error node, so a prefix check would wrongly mark it clean.
///
/// Instead we neutralize only the suspected limitation and re-parse the whole
/// file: replace the `[]` array suffix on every import-type member with a single
/// space (leaving the scalar import-type, which parses clean in every position)
/// and re-parse. The replacement is a SPACE rather than a deletion so it is
/// token-preserving: deleting an empty `[]` between an import-type member and a
/// trailing identifier fragment (`import('x').Sub[]scription`) would glue them
/// into one valid identifier and falsely excuse a broken file; a space keeps
/// them as two tokens, so the broken file stays broken.
/// We return `true` iff:
///   1. the language is TypeScript, AND
///   2. the original source genuinely has a parse error, AND
///   3. at least one import-type-array construct is present (the regex matched),
///      AND
///   4. after stripping ONLY those array suffixes the file parses completely
///      clean (no ERROR/MISSING node anywhere).
///
/// Because the transform changes nothing but the array suffix (replaced by a
/// single space), a clean re-parse proves the array suffix was the SOLE cause of
/// the error. Any unrelated error elsewhere keeps the transformed parse dirty, so
/// a genuinely broken file stays classified as a partial parse.
pub(crate) fn is_expected_typescript_import_type_array_limitation(
    language: &LanguageId,
    content: &[u8],
    is_tsx: bool,
) -> bool {
    if !matches!(language, LanguageId::TypeScript) {
        return false;
    }

    // Memoized: the regex pre-gate plus up to two full re-parses below run at
    // most once per distinct content (see `expected_partial_memo`); render
    // paths calling this repeatedly with unchanged content hit the cache.
    expected_partial_memo(ExpectedPartialCheck::TsImportTypeArray, content, || {
        let source = String::from_utf8_lossy(content);

        // The construct must actually be present; otherwise this limitation is not
        // what we are looking at.
        if !IMPORT_TYPE_ARRAY_SUFFIX_RE.is_match(&source) {
            return false;
        }

        // Defensive: only meaningful for files that genuinely failed to parse. If
        // the original parses clean there is no limitation to excuse.
        let original_has_error =
            match panic::catch_unwind(|| parse_source(&source, language, is_tsx)) {
                Ok(Ok((_, has_error, _, _, _))) => has_error,
                _ => return false,
            };
        if !original_has_error {
            return false;
        }

        // Neutralize ONLY the array suffix on import-type members, then re-parse the
        // whole file. A clean re-parse proves the array suffix was the sole cause.
        //
        // The `[]` run is replaced with a SINGLE SPACE (not deleted) so the
        // neutralization is token-preserving. Deleting an empty `[]` between an
        // import-type member and a trailing identifier fragment would GLUE them into
        // a single valid identifier (e.g. `import('x').Sub[]scription` ->
        // `import('x').Subscription`), making a genuinely broken file re-parse clean
        // and be falsely excused. A space keeps `Sub[]scription` as two tokens
        // (`Sub scription`, still broken) while the legitimate `Subscription[]`
        // becomes `Subscription ` (a scalar import-type with trailing whitespace,
        // still clean). Multi-dim `[][]` is one match, replaced by one space.
        let neutralized = IMPORT_TYPE_ARRAY_SUFFIX_RE.replace_all(&source, "${1} ");
        match panic::catch_unwind(|| parse_source(&neutralized, language, is_tsx)) {
            Ok(Ok((_, has_error, _, _, _))) => !has_error,
            _ => false,
        }
    })
}

/// Matches an Angular control-flow opener (`@if`/`@for`/`@switch`/`@defer`/
/// `@else if`) together with its parenthesized control expression. Capture
/// groups:
///   1. the boundary char before `@` (start-of-line or a non-identifier char),
///      preserved so we never match a keyword embedded in an identifier (e.g.
///      `foo@if`),
///   2. the opener keyword plus the opening `(`,
///   3. the control expression body (no nested parens — `[^()]*` keeps the
///      match scoped to a single, balanced opener expression),
///   4. the closing `)`.
///
/// Only the relational operators (`<`/`>`, covering `<`, `>`, `<=`, `>=`) inside
/// group 3 are the `tree-sitter-html 0.23.2` grammar trigger (SF-004): the `>` in
/// `@if (a > b) {` is lexed as a tag close, producing an ERROR node. Neutralizing
/// those operators within group 3 only — never elsewhere in the file — lets us
/// re-parse the whole file and prove whether the operators were the SOLE cause.
static ANGULAR_CONTROL_FLOW_OPENER_RE: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| {
        regex::Regex::new(
            r"(?m)(^|[^A-Za-z0-9_$])(@(?:if|for|switch|defer|else\s+if)\s*\()([^()]*)(\))",
        )
        .expect("SF-004 Angular control-flow opener regex is a valid pattern")
    });

/// SF-004: recognize a partial parse whose ONLY cause is the known
/// `tree-sitter-html 0.23.2` grammar limitation on Angular template control-flow
/// relational operators (`@if (a > b) {`, `@for`, `@switch`, `@defer`,
/// `@else if`). `tree-sitter-html` has zero Angular rules; the `<`/`>` relational
/// operator inside a control expression is lexed as a tag delimiter, producing an
/// ERROR node even though SymForge text-scans the construct and still extracts
/// symbols.
///
/// Soundness — this validates the WHOLE file, not a single diagnostic line. The
/// previous heuristic trusted `parse_diagnostic.line` to point at the Angular
/// opener; that is UNSOUND, because tree-sitter's deepest-smallest ERROR node can
/// pin the diagnostic to a valid `@if` line even when the real defect is an
/// unclosed `<div>` or a stray `</div>` elsewhere (verified empirically against
/// tree-sitter-html 0.23.2: a stray end tag after a closed structure reports its
/// error on the `@if` opener line, masking the real defect). A no-diagnostic
/// fallback was even worse — it excused arbitrary broken HTML.
///
/// Instead we mirror SF-003: neutralize ONLY the suspected limitation and
/// re-parse the whole file. For every Angular control-flow opener we replace each
/// `<`/`>` inside its `(...)` control expression with a single space (length- and
/// token-preserving; a space cannot fuse adjacent tokens), leaving the rest of the
/// file — including every ordinary HTML tag's `<`/`>` — byte-for-byte unchanged.
/// We return `true` iff:
///   1. the language is HTML, AND
///   2. the original source genuinely has a parse error, AND
///   3. at least one Angular control-flow opener is present (the regex matched),
///      AND
///   4. after neutralizing ONLY those openers' relational operators the file
///      parses completely clean (no ERROR/MISSING node anywhere).
///
/// Because the transform changes nothing but the relational operators inside the
/// Angular openers, a clean re-parse proves those operators were the SOLE cause of
/// the error. Any unrelated defect (unclosed `<div>`, stray `</div>`/
/// erroneous_end_tag, broken attribute anywhere) keeps the transformed parse dirty,
/// so a genuinely broken file is NOT excused. This closes both the masked-defect
/// hole and the no-diagnostic-fallback hole by construction.
///
/// Known limitation (safe direction): `tree-sitter-html` also mis-lexes `&&`
/// inside a control expression, and we do not neutralize it. A valid Angular
/// template whose control expression uses `&&` therefore stays dirty after
/// neutralization and is NOT excused — it surfaces as an unexpected partial rather
/// than being masked. Under-excusing is the safe failure mode; we never falsely
/// excuse a broken file.
pub(crate) fn is_expected_angular_template_control_flow_limitation(
    language: &LanguageId,
    content: &[u8],
) -> bool {
    if !matches!(language, LanguageId::Html) {
        return false;
    }

    // Memoized: the regex pre-gate plus up to two full re-parses below run at
    // most once per distinct content (see `expected_partial_memo`); render
    // paths calling this repeatedly with unchanged content hit the cache.
    expected_partial_memo(ExpectedPartialCheck::AngularControlFlow, content, || {
        let source = String::from_utf8_lossy(content);

        // The construct must actually be present; otherwise this limitation is not
        // what we are looking at.
        if !ANGULAR_CONTROL_FLOW_OPENER_RE.is_match(&source) {
            return false;
        }

        // Defensive: only meaningful for files that genuinely failed to parse. If the
        // original parses clean there is no limitation to excuse.
        // HTML is never TSX; the flavor flag is irrelevant for this grammar.
        let original_has_error =
            match panic::catch_unwind(|| parse_source(&source, language, false)) {
                Ok(Ok((_, has_error, _, _, _))) => has_error,
                _ => return false,
            };
        if !original_has_error {
            return false;
        }

        // Neutralize ONLY the relational operators inside each Angular control-flow
        // opener's `(...)` expression, then re-parse the whole file. Each `<`/`>` in
        // capture group 3 becomes a single space; the opener keyword, the parens, and
        // every byte outside the matched openers (including ordinary HTML tag `<`/`>`)
        // are preserved verbatim. A clean re-parse proves the relational operators were
        // the sole cause.
        let neutralized =
            ANGULAR_CONTROL_FLOW_OPENER_RE.replace_all(&source, |caps: &regex::Captures| {
                format!(
                    "{}{}{}{}",
                    &caps[1],
                    &caps[2],
                    caps[3].replace(['<', '>'], " "),
                    &caps[4],
                )
            });
        match panic::catch_unwind(|| parse_source(&neutralized, language, false)) {
            Ok(Ok((_, has_error, _, _, _))) => !has_error,
            _ => false,
        }
    })
}

/// Extract symbol name → body-hash pairs from source code using tree-sitter.
///
/// Used by `diff_symbols` to compare symbol-level changes between git refs.
/// Falls back to `None` for unsupported or config languages so callers can
/// use the legacy regex extractor.
pub fn extract_symbols_for_diff(source: &str, path: &str) -> Option<Vec<(String, String)>> {
    let ext = path.rsplit('.').next().unwrap_or("");
    let language = LanguageId::from_extension(ext)?;
    if config_extractors::is_config_language(&language) {
        return None; // Config files don't go through tree-sitter.
    }
    let is_tsx = LanguageId::is_tsx_path(path);
    let result = panic::catch_unwind(|| parse_source(source, &language, is_tsx));
    let (symbols, ..) = match result {
        Ok(Ok(output)) => output,
        _ => return None,
    };
    let pairs: Vec<(String, String)> = symbols
        .iter()
        .map(|sym| {
            let (start, end) = sym.byte_range;
            let body = &source[start as usize..end as usize];
            (sym.name.clone(), crate::hash::digest_hex(body.as_bytes()))
        })
        .collect();
    Some(pairs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{FileOutcome, LanguageId, SymbolKind};

    #[test]
    fn test_process_file_rust_extracts_function() {
        let source = b"fn hello() { }";
        let result = process_file("test.rs", source, LanguageId::Rust);
        assert_eq!(result.outcome, FileOutcome::Processed);
        assert!(!result.symbols.is_empty());
        assert_eq!(result.symbols[0].name, "hello");
        assert_eq!(result.symbols[0].kind, SymbolKind::Function);
    }

    // Regression guard: tree-sitter-rust must parse `&raw` as a borrow of a
    // variable named `raw`, not as the start of `&raw const`/`&raw mut`.
    // Originally guarded an in-tree byte-rewrite workaround (043b884); now
    // pins the upgraded parser (9f7ff32) against future grammar regressions.
    #[test]
    fn test_process_file_rust_accepts_borrowed_raw_identifier() {
        let source = b"fn main() { let raw = 1; let _x = &raw; }";
        let result = process_file("test.rs", source, LanguageId::Rust);
        assert_eq!(result.outcome, FileOutcome::Processed);
    }

    // Regression guard: tree-sitter-rust must still recognize Rust 2024
    // raw-reference syntax after the upgrade.
    #[test]
    fn test_process_file_rust_preserves_raw_borrow_syntax() {
        for source in [
            b"fn main() { let value = 1; let _ptr = &raw const value; }" as &[u8],
            b"fn main() { let mut value = 1; let _ptr = &raw mut value; }",
        ] {
            let result = process_file("test.rs", source, LanguageId::Rust);
            assert_eq!(result.outcome, FileOutcome::Processed);
        }
    }

    #[test]
    fn test_process_file_python_extracts_function() {
        let source = b"def greet():\n    pass";
        let result = process_file("test.py", source, LanguageId::Python);
        assert_eq!(result.outcome, FileOutcome::Processed);
        assert!(!result.symbols.is_empty());
        assert_eq!(result.symbols[0].name, "greet");
        assert_eq!(result.symbols[0].kind, SymbolKind::Function);
    }

    #[test]
    fn test_process_file_javascript_extracts_function() {
        let source = b"function doStuff() { }";
        let result = process_file("test.js", source, LanguageId::JavaScript);
        assert_eq!(result.outcome, FileOutcome::Processed);
        assert!(!result.symbols.is_empty());
        assert_eq!(result.symbols[0].name, "doStuff");
        assert_eq!(result.symbols[0].kind, SymbolKind::Function);
    }

    #[test]
    fn test_process_file_typescript_extracts_interface() {
        let source = b"interface Greeter { greet(): void; }";
        let result = process_file("test.ts", source, LanguageId::TypeScript);
        assert_eq!(result.outcome, FileOutcome::Processed);
        let interface = result
            .symbols
            .iter()
            .find(|s| s.kind == SymbolKind::Interface);
        assert!(interface.is_some());
        assert_eq!(interface.unwrap().name, "Greeter");
    }

    // SF-003: tree-sitter-typescript 0.23.2 mis-parses an import-type immediately
    // followed by `[]` (valid TS). The detector must recognize this known grammar
    // limitation WITHOUT over-broadening to mask genuine syntax errors.

    #[test]
    fn test_sf003_class_field_import_type_array_is_expected_limitation() {
        // The exact reported repro shape: `private subs: import('rxjs').Subscription[] = [];`
        let source = b"class C { private subs: import('rxjs').Subscription[] = []; }";
        let result = process_file(
            "workflow-builder.component.ts",
            source,
            LanguageId::TypeScript,
        );
        // tree-sitter still flags it as a partial parse...
        assert!(
            matches!(result.outcome, FileOutcome::PartialParse { .. }),
            "expected grammar still reports a partial parse for the limitation case"
        );
        // ...but the SF-003 detector recognizes it as the known grammar limitation.
        assert!(
            is_expected_typescript_import_type_array_limitation(
                &LanguageId::TypeScript,
                source,
                false
            ),
            "class-field import-type-array must be recognized as an expected grammar limitation"
        );
        // Symbols are still extracted (the class `C` is present).
        assert!(
            result.symbols.iter().any(|s| s.name == "C"),
            "class symbol C must still be extracted despite the partial parse"
        );
    }

    #[test]
    fn test_sf003_type_alias_import_type_array_is_expected_limitation() {
        // The variant a naive `[`-prefix detector would miss (the error node here is
        // a MISSING `;`, not the import-type itself).
        let source = b"type S = import('rxjs').Subscription[];";
        let result = process_file("types.ts", source, LanguageId::TypeScript);
        assert!(
            matches!(result.outcome, FileOutcome::PartialParse { .. }),
            "type-alias import-type-array still reports a partial parse"
        );
        assert!(
            is_expected_typescript_import_type_array_limitation(
                &LanguageId::TypeScript,
                source,
                false
            ),
            "type-alias import-type-array must be recognized as an expected grammar limitation"
        );
    }

    #[test]
    fn test_sf003_multidim_import_type_array_is_expected_limitation() {
        let source = b"type S = import('rxjs').Subscription[][];";
        assert!(
            is_expected_typescript_import_type_array_limitation(
                &LanguageId::TypeScript,
                source,
                false
            ),
            "multi-dimensional import-type-array must be recognized as an expected limitation"
        );
    }

    /// Perf regression (review finding 1, post-v7.19.0): the expensive
    /// neutralize-and-reparse classification must be computed at most once per
    /// distinct content. The second lookup's compute closure panics, so this
    /// fails loudly if the memo stops short-circuiting; distinct check kinds
    /// and distinct content must not share verdicts.
    #[test]
    fn test_expected_partial_memo_hit_skips_recompute() {
        // Unique content so parallel/preceding tests cannot pre-seed the key.
        let content = b"memo-probe :: post-v7.19.0 review fix 1 :: unique";
        assert!(expected_partial_memo(
            ExpectedPartialCheck::TsImportTypeArray,
            content,
            || true
        ));
        // Memo hit: the closure must NOT run again for the same (check, content).
        assert!(expected_partial_memo(
            ExpectedPartialCheck::TsImportTypeArray,
            content,
            || panic!("verdict must come from the memo, not a recompute"),
        ));
        // The OTHER check kind on the same content is a distinct key.
        assert!(!expected_partial_memo(
            ExpectedPartialCheck::AngularControlFlow,
            content,
            || false
        ));
        // Different content (same length as `content`) is a distinct key.
        let other = b"memo-probe :: post-v7.19.0 review fix 1 :: uniquf";
        assert_eq!(content.len(), other.len());
        assert!(!expected_partial_memo(
            ExpectedPartialCheck::TsImportTypeArray,
            other,
            || false
        ));
    }

    #[test]
    fn test_sf003_negative_control_genuinely_broken_array_stays_partial() {
        // REQUIRED negative control: a genuinely broken file that begins with the
        // SAME import-type-array prefix (byte-identical error node) MUST NOT be
        // excused. This proves the detector validates the WHOLE construct, not just
        // the error-node prefix.
        let source = b"class C { private subs: import('rxjs').Subscription[] = [ ; foo bar baz }";
        let result = process_file("broken.ts", source, LanguageId::TypeScript);
        assert!(
            matches!(result.outcome, FileOutcome::PartialParse { .. }),
            "the genuinely broken file is a partial parse"
        );
        assert!(
            !is_expected_typescript_import_type_array_limitation(
                &LanguageId::TypeScript,
                source,
                false
            ),
            "a genuinely broken file sharing the import-type-array prefix MUST stay partial"
        );
    }

    #[test]
    fn test_sf003_negative_control_identifier_glue_stays_partial() {
        // SOUNDNESS REGRESSION (glue bypass): a genuinely broken file where an
        // EMPTY `[]` sits between an import-type member and a trailing identifier
        // fragment. A token-destroying neutralization (deleting the `[]`) would
        // fuse `Sub[]scription` into the valid identifier `Subscription` and falsely
        // re-parse clean. The token-preserving (space) neutralization keeps it as
        // `Sub scription` (two tokens), so it stays partial and is NOT excused.
        let source = b"type S = import('x').Sub[]scription;";
        let result = process_file("glue-type-alias.ts", source, LanguageId::TypeScript);
        assert!(
            matches!(result.outcome, FileOutcome::PartialParse { .. }),
            "the genuinely broken identifier-glue file is a partial parse"
        );
        assert!(
            !is_expected_typescript_import_type_array_limitation(
                &LanguageId::TypeScript,
                source,
                false
            ),
            "deleting `[]` would glue `Sub[]scription` into a valid identifier; the \
             token-preserving neutralization must keep this broken file partial"
        );
    }

    #[test]
    fn test_sf003_negative_control_identifier_glue_class_field_stays_partial() {
        // Same glue bypass in a class-field position (the reported repro shape):
        // `import('rxjs').Sub[]scription` must NOT be excused.
        let source = b"class C { private a: import('rxjs').Sub[]scription = x; }";
        let result = process_file("glue-class-field.ts", source, LanguageId::TypeScript);
        assert!(
            matches!(result.outcome, FileOutcome::PartialParse { .. }),
            "the genuinely broken class-field identifier-glue file is a partial parse"
        );
        assert!(
            !is_expected_typescript_import_type_array_limitation(
                &LanguageId::TypeScript,
                source,
                false
            ),
            "a class-field import-type member glued to a trailing identifier fragment \
             must stay partial under the token-preserving neutralization"
        );
    }

    #[test]
    fn test_sf003_negative_control_unrelated_syntax_error_stays_partial() {
        // A plain malformed class with no import-type-array at all stays partial.
        let source = b"class C { private x: = ; }";
        assert!(
            !is_expected_typescript_import_type_array_limitation(
                &LanguageId::TypeScript,
                source,
                false
            ),
            "an unrelated syntax error must not be excused as an import-type-array limitation"
        );
    }

    #[test]
    fn test_sf003_negative_control_valid_array_plus_real_error_stays_partial() {
        // A valid import-type-array PLUS a separate real error elsewhere: neutralizing
        // the array suffix does NOT make the file clean, so it stays partial. The
        // detector never masks the real defect.
        let source = b"type S = import('rxjs').Subscription[]; class C { private x: = ; }";
        assert!(
            !is_expected_typescript_import_type_array_limitation(
                &LanguageId::TypeScript,
                source,
                false
            ),
            "a real error elsewhere must keep the file partial even when an import-type-array is present"
        );
    }

    #[test]
    fn test_sf003_detector_is_typescript_gated() {
        // The detector only applies to TypeScript. Other languages are never excused.
        let source = b"type S = import('rxjs').Subscription[];";
        assert!(
            !is_expected_typescript_import_type_array_limitation(
                &LanguageId::JavaScript,
                source,
                false
            ),
            "the import-type-array limitation detector must be TypeScript-gated"
        );
    }

    #[test]
    fn test_sf003_detector_false_on_clean_typescript() {
        // A clean TS file with no import-type-array is never flagged as the limitation.
        let source = b"type S = import('rxjs').Subscription;";
        assert!(
            !is_expected_typescript_import_type_array_limitation(
                &LanguageId::TypeScript,
                source,
                false
            ),
            "clean scalar import-type must not be classified as a limitation"
        );
    }

    // --- TSX grammar selection (.tsx uses LANGUAGE_TSX, .ts stays on TYPESCRIPT) ---

    #[test]
    fn test_tsx_jsx_component_parses_clean_and_extracts_symbols() {
        // The real-world repro: a JSX-returning function. The plain TypeScript
        // grammar cannot parse JSX and reports a partial parse ("syntax missing
        // >"); the TSX grammar (selected from the `.tsx` extension) parses it
        // cleanly and the JSX-nested function symbols survive.
        let source = br#"
export function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>
  );
}
"#;
        let result = process_file("src/App.tsx", source, LanguageId::TypeScript);
        assert_eq!(
            result.outcome,
            FileOutcome::Processed,
            "a `.tsx` JSX component must parse cleanly under the TSX grammar; got {:?} / {:?}",
            result.outcome,
            result.parse_diagnostic
        );
        assert!(
            result.parse_diagnostic.is_none(),
            "clean TSX parse must not attach a diagnostic; got {:?}",
            result.parse_diagnostic
        );
        assert!(
            result.symbols.iter().any(|s| s.name == "App"),
            "the JSX component function `App` must be extracted; got {:?}",
            result.symbols
        );
    }

    #[test]
    fn test_tsx_grammar_selected_by_extension_not_languageid() {
        // The same JSX source under a `.ts` extension uses the plain TypeScript
        // grammar, which cannot parse JSX — so it is a partial parse. This pins
        // the invariant that the grammar is chosen by the file extension, not by
        // the (shared) LanguageId::TypeScript.
        let source = br#"
export function App() {
  return <div>hi</div>;
}
"#;
        let as_tsx = process_file("src/App.tsx", source, LanguageId::TypeScript);
        assert_eq!(
            as_tsx.outcome,
            FileOutcome::Processed,
            "JSX under `.tsx` must parse clean"
        );
        let as_ts = process_file("src/App.ts", source, LanguageId::TypeScript);
        assert!(
            matches!(as_ts.outcome, FileOutcome::PartialParse { .. }),
            "JSX under `.ts` must remain a partial parse (plain TS grammar has no JSX); got {:?}",
            as_ts.outcome
        );
    }

    #[test]
    fn test_ts_angle_bracket_type_assertion_still_parses_clean() {
        // CRITICAL regression: the TSX grammar rejects legacy angle-bracket type
        // assertions (`<T>expr`), which are valid in plain `.ts`. A `.ts` file
        // using `<number>y` must keep parsing cleanly on LANGUAGE_TYPESCRIPT and
        // must NOT be routed to the TSX grammar.
        let source = b"const y: unknown = 1;\nconst x = <number>y;\n";
        let result = process_file("src/cast.ts", source, LanguageId::TypeScript);
        assert_eq!(
            result.outcome,
            FileOutcome::Processed,
            "a `.ts` angle-bracket cast must parse cleanly under the TypeScript grammar; got {:?} / {:?}",
            result.outcome,
            result.parse_diagnostic
        );
        assert!(
            result.parse_diagnostic.is_none(),
            "clean `.ts` cast must not attach a diagnostic; got {:?}",
            result.parse_diagnostic
        );
        assert!(
            result.symbols.iter().any(|s| s.name == "x"),
            "the cast binding `x` must be extracted; got {:?}",
            result.symbols
        );
    }

    #[test]
    fn test_is_tsx_path_classifier() {
        assert!(LanguageId::is_tsx_path("src/App.tsx"));
        assert!(LanguageId::is_tsx_path("App.TSX"));
        assert!(LanguageId::is_tsx_path(r"src\components\App.tsx"));
        assert!(!LanguageId::is_tsx_path("src/App.ts"));
        assert!(!LanguageId::is_tsx_path("src/App.jsx"));
        assert!(!LanguageId::is_tsx_path("tsx"));
        assert!(!LanguageId::is_tsx_path("dir.tsx/file.ts"));
        assert!(!LanguageId::is_tsx_path("noext"));
    }

    #[test]
    fn test_process_file_go_extracts_function() {
        let source = b"package main\nfunc main() { }";
        let result = process_file("test.go", source, LanguageId::Go);
        assert_eq!(result.outcome, FileOutcome::Processed);
        assert!(!result.symbols.is_empty());
        assert_eq!(result.symbols[0].name, "main");
        assert_eq!(result.symbols[0].kind, SymbolKind::Function);
    }

    #[test]
    fn test_process_file_partial_parse() {
        let source = b"fn broken( { }";
        let result = process_file("bad.rs", source, LanguageId::Rust);
        assert!(matches!(result.outcome, FileOutcome::PartialParse { .. }));
    }

    #[test]
    fn test_process_file_partial_parse_has_diagnostic() {
        let source = b"fn broken( { }";
        let result = process_file("bad.rs", source, LanguageId::Rust);
        assert!(matches!(result.outcome, FileOutcome::PartialParse { .. }));
        let diag = result
            .parse_diagnostic
            .expect("should have a diagnostic for partial parse");
        assert_eq!(diag.parser, "tree-sitter");
        assert!(diag.line.is_some(), "diagnostic should have a line number");
        assert!(
            diag.column.is_some(),
            "diagnostic should have a column number"
        );
        assert!(
            diag.byte_span.is_some(),
            "diagnostic should have a byte span"
        );
        assert!(
            diag.message.contains("syntax"),
            "message should describe the error"
        );
    }

    #[test]
    fn test_process_file_partial_parse_diagnostic_pins_location() {
        // Tightens the ParseDiagnostic contract that validate_file_syntax relies on:
        // a partial parse must pinpoint the actually-broken line, not just return
        // Some(1) for everything. The source below has two clean lines followed
        // by a broken one, so a regression that loses multi-line tracking or
        // hardcodes line 1 would slip past the existing is_some()-only test but
        // fail here.
        let source = b"fn foo() {}\nfn bar() {}\nfn broken( { }";
        let result = process_file("multi.rs", source, LanguageId::Rust);

        assert!(
            matches!(result.outcome, FileOutcome::PartialParse { .. }),
            "source with a line-3 syntax error should be partial-parsed; got {:?}",
            result.outcome
        );

        let diag = result
            .parse_diagnostic
            .expect("partial parse must attach a ParseDiagnostic");

        assert_eq!(diag.parser, "tree-sitter");
        assert!(
            !diag.fallback_used,
            "tree-sitter parses must not set fallback_used; that flag is reserved \
             for config extractors that recover via a secondary parser"
        );

        // Line is 1-based and must track the actual error row. Line 3 is where
        // `fn broken( { }` lives (bytes 24..).
        let line = diag.line.expect("diagnostic must carry a line number");
        assert!(
            line >= 3,
            "error is on line 3 of the source; diagnostic reported line {line}"
        );

        let column = diag.column.expect("diagnostic must carry a column number");
        assert!(column >= 1, "columns are 1-based; got {column}");

        // Byte span must be ordered and inside the source, and must land on
        // line 3 (which starts at byte 24: "fn foo() {}\n" + "fn bar() {}\n").
        // Note: tree-sitter MISSING nodes are zero-width (start == end), so we
        // allow span_start == span_end but require start <= end.
        let (span_start, span_end) = diag
            .byte_span
            .expect("diagnostic must carry a byte span for downstream editors");
        assert!(
            span_start <= span_end,
            "byte_span must be ordered; got {span_start}..{span_end}"
        );
        assert!(
            (span_end as usize) <= source.len(),
            "byte_span must fit inside source (len {}); got end {span_end}",
            source.len()
        );
        assert!(
            span_start >= 24,
            "byte_span should point at line 3 content (starts at byte 24); got {span_start}"
        );

        // location_display is what validate_file_syntax / get_file_context use
        // to render "(line X, column Y)" in tool output. Both must flow through.
        let loc = diag
            .location_display()
            .expect("location_display must render when both line and column are present");
        assert!(
            loc.contains(&format!("line {line}")),
            "location_display must include the line; got {loc}"
        );
        assert!(
            loc.contains(&format!("column {column}")),
            "location_display must include the column; got {loc}"
        );

        // summary() is what feeds into FileOutcome::PartialParse { warning } —
        // pin that the warning carries the structured location, not just the bare
        // message, so the index-health "partial files" path shows actionable info.
        let summary = diag.summary();
        assert!(
            summary.contains("tree-sitter:"),
            "summary should prefix with parser name; got {summary}"
        );
        assert!(
            summary.contains(&format!("line {line}")),
            "summary should include location for downstream display; got {summary}"
        );
    }

    #[test]
    fn test_parse_source_reports_deepest_actionable_nested_error_node() {
        let source =
            "def compute():\n    value = outer(inner(1 + 2, tail)\n    other = {'a': {'b': 1}}\n";
        let (_symbols, has_error, diagnostic, _references, _aliases) =
            parse_source(source, &LanguageId::Python, false).expect("python parse should complete");

        assert!(has_error, "fixture must contain a tree-sitter parse error");
        let diag = diagnostic.expect("partial parse must attach a diagnostic");

        assert_eq!(diag.parser, "tree-sitter");
        assert_eq!(diag.line, Some(2), "unexpected diagnostic: {diag:?}");
        assert_eq!(diag.column, Some(19), "unexpected diagnostic: {diag:?}");
        assert_eq!(
            diag.byte_span,
            Some((33, 51)),
            "diagnostic must pin the nested call error, not the outer assignment; got {diag:?}"
        );
        assert!(
            diag.message
                .contains("syntax error near `inner(1 + 2, tail)`"),
            "diagnostic should include the nested call context; got {diag:?}"
        );
    }

    #[test]
    fn test_parse_source_reports_actionable_missing_node_context() {
        let source = "fn compute() {\n    let value = outer(inner(1 + ), tail);\n}\n";
        let (_symbols, has_error, diagnostic, _references, _aliases) =
            parse_source(source, &LanguageId::Rust, false).expect("rust parse should complete");

        assert!(has_error, "fixture must contain a tree-sitter parse error");
        let diag = diagnostic.expect("partial parse must attach a diagnostic");

        assert_eq!(diag.parser, "tree-sitter");
        assert_eq!(diag.line, Some(2), "unexpected diagnostic: {diag:?}");
        assert_eq!(diag.column, Some(32), "unexpected diagnostic: {diag:?}");
        assert_eq!(
            diag.byte_span,
            Some((46, 46)),
            "diagnostic must pin the zero-width missing node; got {diag:?}"
        );
        assert!(
            diag.message.contains("syntax missing identifier"),
            "diagnostic should name the missing node kind; got {diag:?}"
        );
        assert!(
            diag.message.contains("), tail"),
            "diagnostic should include source context after the missing node; got {diag:?}"
        );
    }

    #[test]
    fn test_process_file_computes_content_hash() {
        let source = b"fn foo() {}";
        let result = process_file("hash_test.rs", source, LanguageId::Rust);
        assert!(!result.content_hash.is_empty());
        assert_eq!(result.content_hash, digest_hex(source));
    }

    #[test]
    fn test_process_file_byte_len() {
        let source = b"fn bar() {}";
        let result = process_file("len.rs", source, LanguageId::Rust);
        assert_eq!(result.byte_len, source.len() as u64);
    }

    #[test]
    fn test_process_file_preserves_relative_path() {
        let result = process_file("src/lib.rs", b"fn x() {}", LanguageId::Rust);
        assert_eq!(result.relative_path, "src/lib.rs");
    }

    #[test]
    fn test_process_file_never_panics_on_adversarial_input() {
        // Verifies the catch_unwind safety net: process_file must ALWAYS
        // return a FileProcessingResult regardless of input, never propagate a panic.
        let cases: &[(&[u8], &str, LanguageId)] = &[
            (b"\xff\xfe\x00\x01", "binary.rs", LanguageId::Rust),
            (b"", "empty.py", LanguageId::Python),
            (&[0u8; 10000], "zeros.js", LanguageId::JavaScript),
            (b"\n\n\n\n\n", "newlines.ts", LanguageId::TypeScript),
            ("\u{200b}\u{200b}".as_bytes(), "zwsp.go", LanguageId::Go),
            (
                b"\0\0\0fn main() {}\0\0",
                "null_padded.rs",
                LanguageId::Rust,
            ),
        ];

        for &(source, path, ref lang) in cases {
            let result = process_file(path, source, lang.clone());
            assert_eq!(result.relative_path, path);
            assert_eq!(result.byte_len, source.len() as u64);
            assert!(!result.content_hash.is_empty());
        }
    }

    #[test]
    fn test_process_file_ruby_extracts_method() {
        let source = b"def hello\n  puts 'hi'\nend";
        let result = process_file("app.rb", source, LanguageId::Ruby);
        assert_eq!(result.outcome, FileOutcome::Processed);
        assert!(
            !result.symbols.is_empty(),
            "should have symbols for Ruby source"
        );
    }

    #[test]
    fn test_process_file_is_idempotent_across_re_parses() {
        // Incremental re-indexing in src/watcher/ assumes `process_file` is
        // idempotent: identical bytes must yield an identical
        // `FileProcessingResult`. A subtle non-determinism (e.g. HashMap
        // iteration order leaking into the symbol/reference Vec ordering)
        // would cause phantom xref churn on every touch of an unchanged file.
        //
        // Exercises multiple languages plus a partial-parse case so both the
        // normal extractor path and the `collect_first_error_node` diagnostic
        // path are pinned.
        let rust_src: &[u8] = b"use std::collections::HashMap;\n\
            use std::fmt::Display;\n\
            \n\
            pub struct Cache<K, V> {\n\
                store: HashMap<K, V>,\n\
            }\n\
            \n\
            impl<K: std::hash::Hash + Eq, V> Cache<K, V> {\n\
                pub fn new() -> Self { Self { store: HashMap::new() } }\n\
                pub fn insert(&mut self, k: K, v: V) -> Option<V> { self.store.insert(k, v) }\n\
            }\n\
            \n\
            pub fn make() -> Cache<String, u32> { Cache::new() }\n";

        let python_src: &[u8] = b"import os\n\
            import sys\n\
            \n\
            class Greeter:\n\
                def __init__(self, name):\n\
                    self.name = name\n\
            \n\
                def greet(self):\n\
                    return f\"hello {self.name}\"\n\
            \n\
            def main():\n\
                Greeter(\"world\").greet()\n";

        let ts_src: &[u8] = b"interface Greeter { greet(): string; }\n\
            export class Hello implements Greeter {\n\
                constructor(private name: string) {}\n\
                greet() { return `hi ${this.name}`; }\n\
            }\n";

        // Syntactically broken Rust — exercises the partial-parse path so the
        // diagnostic (line/column/byte_span) must also be deterministic.
        let broken_src: &[u8] = b"fn broken( { }";

        let cases: &[(&[u8], &str, LanguageId)] = &[
            (rust_src, "cache.rs", LanguageId::Rust),
            (python_src, "greet.py", LanguageId::Python),
            (ts_src, "hello.ts", LanguageId::TypeScript),
            (broken_src, "broken.rs", LanguageId::Rust),
        ];

        for &(source, path, ref lang) in cases {
            let first = process_file(path, source, lang.clone());
            let second = process_file(path, source, lang.clone());
            let third = process_file(path, source, lang.clone());
            assert_eq!(
                first, second,
                "{path}: identical bytes must yield identical FileProcessingResult (run 1 vs 2)"
            );
            assert_eq!(
                second, third,
                "{path}: identical bytes must yield identical FileProcessingResult (run 2 vs 3)"
            );
        }
    }

    // --- Parser resilience audit (parse_source / collect_first_error_node) ---
    //
    // The adversarial `process_file` test above proves the `catch_unwind` safety
    // net holds end-to-end. The tests below probe `parse_source` directly (no
    // catch_unwind wrapper) so a panic in the parse path fails the test instead
    // of being silently downgraded to a `Failed` outcome.

    #[test]
    fn test_parse_source_zero_bytes() {
        let result = parse_source("", &LanguageId::Rust, false)
            .expect("parse_source must handle empty input without error");
        let (symbols, _has_error, _diagnostic, references, alias_map) = result;
        assert!(symbols.is_empty(), "empty source has no symbols");
        assert!(references.is_empty(), "empty source has no references");
        assert!(alias_map.is_empty(), "empty source has no aliases");
    }

    #[test]
    fn test_parse_source_null_bytes_only() {
        // 4 KiB of NUL: valid UTF-8, no valid tokens in any grammar.
        // Exercises parse_source + collect_first_error_node (if has_error) on a
        // file that tree-sitter must reject-as-syntax-error rather than crash.
        let source: String = "\0".repeat(4096);
        let result = parse_source(&source, &LanguageId::Rust, false)
            .expect("parse_source must handle null-byte input without error");
        let (_symbols, _has_error, _diagnostic, _references, _aliases) = result;
    }

    #[test]
    fn test_parse_source_wide_multibyte_error_region_no_panic() {
        // Regression probe for collect_first_error_node:
        //   let snippet_end = node.end_byte().min(snippet_start + 40);
        //   let snippet = &source[snippet_start..snippet_end];
        //
        // If a multi-byte UTF-8 char straddles byte `snippet_start + 40`, naive
        // slicing panics ("byte index is not a char boundary"). 100 × '€'
        // (3 bytes each = 300 bytes, all non-ASCII) is not a valid Rust token
        // stream, so tree-sitter must produce one or more ERROR nodes that
        // could span > 40 bytes of multi-byte content.
        let source: String = "€".repeat(100);
        parse_source(&source, &LanguageId::Rust, false)
            .expect("parse_source must not panic on wide multi-byte error region");
    }

    #[test]
    fn test_parse_source_mixed_multibyte_error_boundary_no_panic() {
        // Similar char-boundary probe but with a grammar-level syntax error
        // interleaved with multi-byte text — guarantees an ERROR node whose
        // [start, start+40) window crosses a UTF-8 char boundary.
        let mut source = String::new();
        source.push_str("struct S { ");
        for _ in 0..14 {
            source.push('€');
        }
        source.push(' ');
        parse_source(&source, &LanguageId::Rust, false)
            .expect("parse_source must not panic when error snippet spans multi-byte chars");
    }

    #[test]
    fn test_process_file_deeply_nested_expression_no_stack_blow() {
        // Stack-blow probe: 10 000 nested parens. Per-language extractors walk
        // the AST recursively (see `walk_node` → `walk_children` → `walk_node`
        // in src/parsing/languages/rust.rs L19-50). Default Rust test-thread
        // stacks (~2 MiB) overflow around ~6 k frames, so the probe runs on a
        // dedicated thread with a 16 MiB stack to verify the parse logic itself
        // terminates. A real stack-blow on this depth would abort the test
        // process; we want to observe it here rather than in production.
        const DEPTH: usize = 10_000;
        let handle = std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn(|| {
                let mut source = String::with_capacity(DEPTH * 2 + 32);
                source.push_str("fn f() -> i32 { ");
                for _ in 0..DEPTH {
                    source.push('(');
                }
                source.push('1');
                for _ in 0..DEPTH {
                    source.push(')');
                }
                source.push_str(" }");
                let result = process_file("deep.rs", source.as_bytes(), LanguageId::Rust);
                assert_eq!(result.byte_len, source.len() as u64);
                assert!(!result.content_hash.is_empty());
            })
            .expect("spawn stack-blow probe thread");
        handle.join().expect("deep-nesting probe must not panic");
    }

    #[test]
    fn test_process_file_deeply_nested_expression_on_default_stack_no_panic() {
        // Companion to `test_process_file_deeply_nested_expression_no_stack_blow`
        // above — same input, but runs on the default test-thread stack (~2 MiB
        // on Linux/macOS, ~1 MiB on Windows) without the 16 MiB override. The
        // previous swarm round flagged the real risk: daemon-proxy sessions and
        // other caller threads with default-sized stacks would overflow on
        // adversarial input. The `MAX_AST_WALK_DEPTH` cap in
        // `src/parsing/languages/mod.rs` silently truncates the AST walk when
        // recursion reaches the cap, so this now returns cleanly.
        //
        // This test doesn't assert anything about the resulting symbol count —
        // depth-capped partial walks are allowed to drop inner symbols, matching
        // the rest of the parser's partial-parse philosophy. It only asserts
        // that the call returns *at all* without crashing the test process.
        const DEPTH: usize = 10_000;
        let mut source = String::with_capacity(DEPTH * 2 + 32);
        source.push_str("fn f() -> i32 { ");
        for _ in 0..DEPTH {
            source.push('(');
        }
        source.push('1');
        for _ in 0..DEPTH {
            source.push(')');
        }
        source.push_str(" }");
        let result = process_file("deep_default.rs", source.as_bytes(), LanguageId::Rust);
        assert_eq!(result.byte_len, source.len() as u64);
        assert!(!result.content_hash.is_empty());
    }
}
