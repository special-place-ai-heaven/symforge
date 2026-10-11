//! Machine-readable outcome classes and the text classifiers that derive them
//! from a rendered tool answer. Shared by the MCP handlers, the STEL facade
//! and the embedded facade, so one rendered answer classifies one way.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeClass {
    Found,
    NotFound,
    Ambiguous,
    InvalidRequest,
    EmptyResult,
    InternalFailure,
}

impl OutcomeClass {
    pub const ALL: [Self; 6] = [
        Self::Found,
        Self::NotFound,
        Self::Ambiguous,
        Self::InvalidRequest,
        Self::EmptyResult,
        Self::InternalFailure,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Found => "found",
            Self::NotFound => "not_found",
            Self::Ambiguous => "ambiguous",
            Self::InvalidRequest => "invalid_request",
            Self::EmptyResult => "empty_result",
            Self::InternalFailure => "internal_failure",
        }
    }

    pub const fn is_error(self) -> bool {
        matches!(self, Self::InvalidRequest | Self::InternalFailure)
    }
}

pub(crate) fn is_index_unavailable_output(text: &str) -> bool {
    text.starts_with("Index not loaded.")
        || text.starts_with("Index is loading")
        || text.starts_with("Index degraded:")
        || text.starts_with("Index refresh interrupted:")
}

/// The ONE error-shape predicate for the whole protocol surface, read/write
/// alike. `edit_tools.rs` used to keep a private copy of this function, which
/// diverged the moment the project-refusal shapes were added here — so the seven
/// structural edit tools reported a REFUSED edit as `success`/`found`. Shared
/// (`pub(super)`) so a new shape can only ever be taught once.
pub(crate) fn is_error_output(text: &str) -> bool {
    text.starts_with("Error:")
        || text.starts_with("Error in ")
        || is_admission_refusal(text)
        || is_foreign_project_refusal(text)
        || is_local_cross_project_refusal(text)
}

/// An admission-gate refusal is an honest REFUSAL, not a successful read. Taught
/// HERE rather than per-arm: `validate_file_syntax` has no classifier arm of its
/// own, so its refusal fell through `classify_compact_tool_output`'s catch-all
/// and reported a withheld file as a successful validation. Anchored at position
/// 0 (not `contains`) so a body that merely QUOTES the phrase stays `Found`.
/// Both refusal variants share this opening clause, so one anchor classifies
/// both.
pub(crate) fn is_admission_refusal(text: &str) -> bool {
    text.starts_with("Content withheld by admission policy:")
}

/// The single-project refusal [`SymForgeServer::foreign_project_refusal`] emits
/// carries no `Error:` prefix, so without this a refusal that came back through
/// DISPATCH rather than an early return classified as a SUCCESSFUL answer
/// (`found`) — notably in `classify_symforge_edit_outcome` and in every
/// `classify_edit_output` caller. Deliberately narrow: BOTH anchors must match,
/// so no unrelated body is reclassified.
pub(crate) fn is_foreign_project_refusal(text: &str) -> bool {
    // Current shape carries the typed `Error: project_routing:` prefix; the
    // legacy anchor below still classifies pre-10.1 bodies that came back
    // through DISPATCH rather than an early return.
    (text.starts_with("Error: project_routing: project '") || text.starts_with("project '"))
        && text.contains("is not available on this connection")
}

/// Sibling shape emitted by [`SymForgeServer::local_cross_project_refusal`] for
/// a genuinely cross-project (`projects`, `*`, or foreign `project`) read on a
/// transport with no daemon working set. Same defect as above and same fix: an
/// honest refusal must not be reported as a successful answer. Narrow by
/// anchoring the FULL opening clause at position 0: a rendered search hit or doc
/// line that quotes the message is prefixed (`7: // ...`) and stays `Found`.
pub(crate) fn is_local_cross_project_refusal(text: &str) -> bool {
    // Current shape carries the typed `Error: project_routing:` prefix; the
    // legacy anchor still classifies pre-10.1 bodies.
    text.starts_with("Error: project_routing: cross-project queries")
        || text.starts_with("Cross-project queries (project/projects) require the daemon")
}

pub(crate) fn classify_get_symbol_output(text: &str) -> OutcomeClass {
    if is_index_unavailable_output(text) {
        OutcomeClass::InternalFailure
    } else if is_error_output(text) {
        OutcomeClass::InvalidRequest
    } else if text.starts_with("Ambiguous:") {
        OutcomeClass::Ambiguous
    } else if text.starts_with("File not found:") || text.starts_with("No symbol ") {
        OutcomeClass::NotFound
    } else {
        OutcomeClass::Found
    }
}

pub(crate) fn classify_get_symbol_context_output(text: &str) -> OutcomeClass {
    if is_index_unavailable_output(text) {
        OutcomeClass::InternalFailure
    } else if is_error_output(text) {
        OutcomeClass::InvalidRequest
    } else if text.starts_with("Ambiguous symbol selector") || text.starts_with("Ambiguous:") {
        OutcomeClass::Ambiguous
    } else if text.starts_with("Symbol \"")
        || text.starts_with("Symbol '")
        || text.starts_with("Symbol not found in ")
        || text.starts_with("File not found:")
    {
        OutcomeClass::NotFound
    } else {
        OutcomeClass::Found
    }
}

/// Strip an optional leading `── mode: <name> (explicit) ──\n` annotation so
/// output classification can match the renderer's message at the start of the
/// remaining body. Returns `text` unchanged when no annotation is present.
pub(crate) fn strip_mode_annotation(text: &str) -> &str {
    text.strip_prefix("── mode: ")
        .and_then(|rest| rest.split_once(" ──\n"))
        .map(|(_, body)| body)
        .unwrap_or(text)
}
/// The one shape the read renderers emit for a refused selection:
/// `{path} [error: {reason}]`, alone on a single line (`file_read.rs`,
/// `guidance/read_gate.rs`, `protocol/read_gate.rs`). Anchored on that whole
/// shape, not on a bare `[error:` substring, which ordinary source contains.
/// A one-line raw read whose content has exactly this shape is still
/// ambiguous here; `get_file_content` carries a typed outcome so its own status
/// never depends on this.
fn is_path_error_line(body: &str) -> bool {
    !body.contains('\n') && body.ends_with(']') && body.find(" [error: ").is_some_and(|at| at > 0)
}

pub(crate) fn classify_get_file_content_output(text: &str) -> OutcomeClass {
    // Strip an optional `── mode: <name> (explicit) ──` prefix so the renderer's
    // status message is matched at the start of `body` even when an explicit-mode
    // annotation precedes it. Anchoring on `body` (not a bare `contains`) keeps a
    // successful read whose CONTENT merely mentions these phrases classified Found.
    let body = strip_mode_annotation(text);
    if is_index_unavailable_output(text) {
        OutcomeClass::InternalFailure
    } else if is_error_output(text)
        || text.starts_with("Invalid get_file_content request:")
        || text.starts_with("mode=")
        || is_path_error_line(body)
        || body.starts_with("Path is outside the repository root:")
        || (body.starts_with("Chunk ") && body.contains(" out of range for "))
    {
        // A request for a non-existent chunk index is an invalid request, not a
        // successful read: the path exists but the requested page does not.
        OutcomeClass::InvalidRequest
    } else if text.starts_with("Ambiguous symbol selector") {
        OutcomeClass::Ambiguous
    } else if text.starts_with("File not found:")
        || text.starts_with("No symbol ")
        || text.starts_with("Symbol not found in ")
        || body.starts_with("No matches for '")
        || body.starts_with("Match occurrence ")
    {
        // around_match / match-occurrence misses: the needle was not found in the
        // file. These must report NotFound, not a successful read.
        OutcomeClass::NotFound
    } else {
        OutcomeClass::Found
    }
}

pub(crate) fn classify_search_symbols_output(text: &str) -> OutcomeClass {
    if is_index_unavailable_output(text) {
        OutcomeClass::InternalFailure
    } else if is_error_output(text) || text.starts_with("search_symbols requires") {
        OutcomeClass::InvalidRequest
    } else if text.starts_with("No symbols matching") {
        OutcomeClass::EmptyResult
    } else {
        OutcomeClass::Found
    }
}

pub(crate) fn classify_search_text_output(text: &str) -> OutcomeClass {
    if is_index_unavailable_output(text) {
        OutcomeClass::InternalFailure
    } else if is_error_output(text)
        || text.starts_with("Regex search requires")
        || text.starts_with("Search requires")
        || text.starts_with("Invalid regex")
        || text.starts_with("Invalid glob")
        || text.starts_with("whole_word is not supported")
    {
        OutcomeClass::InvalidRequest
    } else if text.starts_with("No matches") || text.starts_with("No AST matches") {
        OutcomeClass::EmptyResult
    } else {
        OutcomeClass::Found
    }
}

pub(crate) fn classify_search_knowledge_output(text: &str) -> OutcomeClass {
    if text.starts_with("Error:") {
        OutcomeClass::InvalidRequest
    } else if text.contains("\nNo match:") {
        OutcomeClass::EmptyResult
    } else if text.starts_with("Readiness:") {
        OutcomeClass::InternalFailure
    } else {
        OutcomeClass::Found
    }
}

pub(crate) fn classify_search_files_output(text: &str) -> OutcomeClass {
    if is_index_unavailable_output(text) {
        OutcomeClass::InternalFailure
    } else if is_error_output(text)
        || text.starts_with("Path search requires")
        || text.starts_with("Path hint must not be empty")
        || text.starts_with("search_files")
    {
        OutcomeClass::InvalidRequest
    } else if text.contains("Ambiguous path hint") {
        OutcomeClass::Ambiguous
    } else if text.starts_with("No indexed source files matching")
        || text.starts_with("No indexed source path matched")
        || text.starts_with("No git history found")
        || text.starts_with("'") && text.contains("not found in git history")
    {
        OutcomeClass::NotFound
    } else if text.starts_with("No high-confidence co-change data") {
        OutcomeClass::EmptyResult
    } else {
        OutcomeClass::Found
    }
}

pub(crate) fn classify_find_references_output(text: &str) -> OutcomeClass {
    if is_index_unavailable_output(text) {
        OutcomeClass::InternalFailure
    } else if is_error_output(text) {
        OutcomeClass::InvalidRequest
    } else if text.starts_with("Ambiguous symbol selector") {
        OutcomeClass::Ambiguous
    } else if text.starts_with("File not found:")
        || text.starts_with("Symbol not found")
        || text.contains("this symbol is not defined in the indexed project")
    {
        OutcomeClass::NotFound
    } else if text.starts_with("No references found")
        || text.starts_with("No implementations found")
    {
        OutcomeClass::EmptyResult
    } else {
        OutcomeClass::Found
    }
}

pub(crate) fn classify_get_file_context_output(text: &str) -> OutcomeClass {
    if is_index_unavailable_output(text) {
        OutcomeClass::InternalFailure
    } else if is_error_output(text) || text.starts_with("Invalid get_file_context") {
        OutcomeClass::InvalidRequest
    } else if text.starts_with("File not found:")
        || text.starts_with("File not found on disk:")
        || text.starts_with("No symbol ")
    {
        OutcomeClass::NotFound
    } else if text.starts_with("Ambiguous") {
        OutcomeClass::Ambiguous
    } else {
        OutcomeClass::Found
    }
}

/// Classify compact-surface legacy tool output for STEL chain admission and replay validation.
pub(crate) fn classify_compact_tool_output(tool: &str, text: &str) -> OutcomeClass {
    match tool {
        "get_symbol" => classify_get_symbol_output(text),
        "get_symbol_context" => classify_get_symbol_context_output(text),
        "get_file_content" => classify_get_file_content_output(text),
        "get_file_context" => classify_get_file_context_output(text),
        "search_symbols" => classify_search_symbols_output(text),
        "search_text" => classify_search_text_output(text),
        "search_knowledge" => classify_search_knowledge_output(text),
        "search_files" => classify_search_files_output(text),
        "find_references" => classify_find_references_output(text),
        _ => {
            if is_index_unavailable_output(text) {
                OutcomeClass::InternalFailure
            } else if is_error_output(text) || text.starts_with("Invalid") {
                OutcomeClass::InvalidRequest
            } else {
                OutcomeClass::Found
            }
        }
    }
}

/// Whether a legacy tool body represents successful serve output for STEL chain continuation.
pub(crate) fn compact_tool_output_is_success(tool: &str, text: &str) -> bool {
    classify_compact_tool_output(tool, text) == OutcomeClass::Found
}

/// One table of resource reads, run through MCP (`resources/read` and the
/// matching `tools/call`) and through the embedded room (resource and query),
/// so both surfaces are held to the same expected outcome class.
#[cfg(test)]
pub(crate) mod read_outcome_cases {
    use super::OutcomeClass;

    /// The fixture workspace every case reads.
    pub(crate) const FILES: &[(&str, &str)] = &[
        (
            "src/lib.rs",
            "pub fn alpha() -> u32 {\n    let marker = \"x [error: not an error]\";\n    1\n}\n\npub fn caller() -> u32 {\n    alpha()\n}\n",
        ),
        ("src/other.rs", "pub fn beta() {}\n"),
        (
            "notes/error.txt",
            "Error: this file starts like an error\nsecond line\n",
        ),
        ("notes/loading.txt", "Index is loading... not really\n"),
        (
            "notes/bracket.txt",
            "notes/bracket.txt [error: imitation]\n",
        ),
        ("notes/miss.txt", "File not found: decoy\nNo symbol here\n"),
    ];

    pub(crate) enum Read {
        /// `raw` turns header and line numbers off so the content leads.
        Content {
            path: &'static str,
            raw: bool,
            around_match: Option<&'static str>,
            match_occurrence: Option<u32>,
            start_line: Option<u32>,
        },
        Context {
            path: &'static str,
        },
        Symbol {
            path: &'static str,
            name: &'static str,
            kind: Option<&'static str>,
        },
        SymbolContext {
            name: &'static str,
            file: Option<&'static str>,
        },
    }

    pub(crate) struct Case {
        pub label: &'static str,
        pub read: Read,
        pub expected: OutcomeClass,
    }

    const fn content(path: &'static str, raw: bool) -> Read {
        Read::Content {
            path,
            raw,
            around_match: None,
            match_occurrence: None,
            start_line: None,
        }
    }

    pub(crate) const CASES: &[Case] = &[
        Case {
            label: "mid-content [error: with header and line numbers",
            read: content("src/lib.rs", false),
            expected: OutcomeClass::Found,
        },
        Case {
            label: "mid-content [error: raw",
            read: content("src/lib.rs", true),
            expected: OutcomeClass::Found,
        },
        Case {
            label: "leading Error: raw",
            read: content("notes/error.txt", true),
            expected: OutcomeClass::Found,
        },
        Case {
            label: "leading Index is loading raw",
            read: content("notes/loading.txt", true),
            expected: OutcomeClass::Found,
        },
        Case {
            label: "leading path [error: line raw",
            read: content("notes/bracket.txt", true),
            expected: OutcomeClass::Found,
        },
        Case {
            label: "leading File not found: and No symbol raw",
            read: content("notes/miss.txt", true),
            expected: OutcomeClass::Found,
        },
        Case {
            label: "missing file",
            read: content("src/missing.rs", false),
            expected: OutcomeClass::NotFound,
        },
        Case {
            label: "around_match miss",
            read: Read::Content {
                path: "src/lib.rs",
                raw: false,
                around_match: Some("zzzz-no-such-text"),
                match_occurrence: None,
                start_line: None,
            },
            expected: OutcomeClass::NotFound,
        },
        Case {
            label: "match_occurrence past the count",
            read: Read::Content {
                path: "src/lib.rs",
                raw: false,
                around_match: Some("alpha"),
                match_occurrence: Some(9),
                start_line: None,
            },
            expected: OutcomeClass::NotFound,
        },
        Case {
            label: "bad line range",
            read: Read::Content {
                path: "src/lib.rs",
                raw: false,
                around_match: None,
                match_occurrence: None,
                start_line: Some(99),
            },
            expected: OutcomeClass::InvalidRequest,
        },
        Case {
            label: "file context of an existing file",
            read: Read::Context { path: "src/lib.rs" },
            expected: OutcomeClass::Found,
        },
        Case {
            label: "file context of a missing file",
            read: Read::Context {
                path: "src/missing.rs",
            },
            expected: OutcomeClass::NotFound,
        },
        Case {
            label: "symbol detail with the right kind",
            read: Read::Symbol {
                path: "src/lib.rs",
                name: "alpha",
                kind: Some("fn"),
            },
            expected: OutcomeClass::Found,
        },
        Case {
            label: "symbol detail with a non-matching kind",
            read: Read::Symbol {
                path: "src/lib.rs",
                name: "alpha",
                kind: Some("struct"),
            },
            expected: OutcomeClass::NotFound,
        },
        Case {
            label: "symbol detail in a missing file",
            read: Read::Symbol {
                path: "src/missing.rs",
                name: "alpha",
                kind: None,
            },
            expected: OutcomeClass::NotFound,
        },
        Case {
            label: "name-only symbol context of an unreferenced symbol",
            read: Read::SymbolContext {
                name: "caller",
                file: None,
            },
            expected: OutcomeClass::Found,
        },
        Case {
            label: "name-only symbol context of a name defined nowhere",
            read: Read::SymbolContext {
                name: "defined_nowhere",
                file: None,
            },
            expected: OutcomeClass::NotFound,
        },
        Case {
            label: "file-filtered symbol context of a symbol defined elsewhere",
            read: Read::SymbolContext {
                name: "alpha",
                file: Some("src/other.rs"),
            },
            expected: OutcomeClass::Found,
        },
        Case {
            label: "file-filtered symbol context of a name defined nowhere",
            read: Read::SymbolContext {
                name: "defined_nowhere",
                file: Some("src/other.rs"),
            },
            expected: OutcomeClass::NotFound,
        },
    ];
}
