//! Canonical 15-op read-only Code Mode function catalog, Python stub renderer,
//! and self-contained agent idioms manual.
//!
//! Mutating operations cannot be represented in [`CodeReadOp`].

use serde::{Deserialize, Serialize};

/// Every host read operation callable inside the Code Mode Python sandbox and `symforge q`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeReadOp {
    SearchSymbols,
    GetSymbol,
    FindReferences,
    FindDependents,
    FileContext,
    FileContent,
    SearchText,
    SearchFiles,
    RepoMap,
    SymbolContext,
    InspectMatch,
    WhatChanged,
    DiffSymbols,
    DetectImpact,
    SearchKnowledge,
}

/// Metadata for one keyword parameter on a sandbox function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParamSpec {
    pub name: &'static str,
    pub py_type: &'static str,
    pub required: bool,
    pub default_repr: &'static str,
    pub doc: &'static str,
}

/// Canonical descriptor for one read-only sandbox function and its `TypedDict` return shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodeOpDescriptor {
    pub op: CodeReadOp,
    pub name: &'static str,
    pub summary: &'static str,
    pub discovery_only: bool,
    pub params: &'static [ParamSpec],
    pub return_type: &'static str,
    pub typed_dicts: &'static str,
    pub keywords: &'static [&'static str],
}

impl CodeReadOp {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SearchSymbols => "search_symbols",
            Self::GetSymbol => "get_symbol",
            Self::FindReferences => "find_references",
            Self::FindDependents => "find_dependents",
            Self::FileContext => "file_context",
            Self::FileContent => "file_content",
            Self::SearchText => "search_text",
            Self::SearchFiles => "search_files",
            Self::RepoMap => "repo_map",
            Self::SymbolContext => "symbol_context",
            Self::InspectMatch => "inspect_match",
            Self::WhatChanged => "what_changed",
            Self::DiffSymbols => "diff_symbols",
            Self::DetectImpact => "detect_impact",
            Self::SearchKnowledge => "search_knowledge",
        }
    }

    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "search_symbols" => Some(Self::SearchSymbols),
            "get_symbol" => Some(Self::GetSymbol),
            "find_references" => Some(Self::FindReferences),
            "find_dependents" => Some(Self::FindDependents),
            "file_context" => Some(Self::FileContext),
            "file_content" => Some(Self::FileContent),
            "search_text" => Some(Self::SearchText),
            "search_files" => Some(Self::SearchFiles),
            "repo_map" => Some(Self::RepoMap),
            "symbol_context" => Some(Self::SymbolContext),
            "inspect_match" => Some(Self::InspectMatch),
            "what_changed" => Some(Self::WhatChanged),
            "diff_symbols" => Some(Self::DiffSymbols),
            "detect_impact" => Some(Self::DetectImpact),
            "search_knowledge" => Some(Self::SearchKnowledge),
            _ => None,
        }
    }

    /// CLI `symforge q` accepts MCP aliases (`get_file_context`, `get_file_content`,
    /// `get_repo_map`, `get_symbol_context`) in addition to canonical sandbox names.
    #[must_use]
    pub fn from_cli_name(name: &str) -> Option<Self> {
        Self::from_name(name).or(match name {
            "get_file_context" => Some(Self::FileContext),
            "get_file_content" => Some(Self::FileContent),
            "get_repo_map" => Some(Self::RepoMap),
            "get_symbol_context" => Some(Self::SymbolContext),
            _ => None,
        })
    }

    #[must_use]
    pub fn descriptor(self) -> &'static CodeOpDescriptor {
        CODE_READ_OPS
            .iter()
            .find(|desc| desc.op == self)
            .expect("every CodeReadOp has a canonical descriptor")
    }

    #[must_use]
    pub fn signature_one_liner(self) -> String {
        render_signature_line(self.descriptor())
    }
}

/// All 15 canonical read-only operations exposed in Code Mode.
pub const CODE_READ_OPS: &[CodeOpDescriptor] = &[
    CodeOpDescriptor {
        op: CodeReadOp::SearchSymbols,
        name: "search_symbols",
        summary: "Find symbols by name, kind, or path prefix (does not bump frecency).",
        discovery_only: true,
        params: &[
            ParamSpec {
                name: "query",
                py_type: "str | None",
                required: false,
                default_repr: "None",
                doc: "Symbol name substring/words; omit to browse.",
            },
            ParamSpec {
                name: "kind",
                py_type: "str | None",
                required: false,
                default_repr: "None",
                doc: "Filter kind: 'fn', 'struct', 'enum', 'trait', 'class', 'method', etc.",
            },
            ParamSpec {
                name: "path_prefix",
                py_type: "str | None",
                required: false,
                default_repr: "None",
                doc: "Restrict to directory/file prefix.",
            },
            ParamSpec {
                name: "language",
                py_type: "str | None",
                required: false,
                default_repr: "None",
                doc: "Language filter, e.g. 'rust', 'python', 'typescript'.",
            },
            ParamSpec {
                name: "limit",
                py_type: "int | None",
                required: false,
                default_repr: "None",
                doc: "Max hits returned (default 50).",
            },
            ParamSpec {
                name: "include_tests",
                py_type: "bool | None",
                required: false,
                default_repr: "None",
                doc: "Include test symbols (default False).",
            },
        ],
        return_type: "list[SymbolHit]",
        typed_dicts: "class SymbolHit(TypedDict):\n    path: str\n    name: str\n    kind: str\n    start_line: int\n    end_line: int\n    tier: str",
        keywords: &[
            "symbol",
            "symbols",
            "function",
            "type",
            "struct",
            "class",
            "name",
            "definition",
            "find",
        ],
    },
    CodeOpDescriptor {
        op: CodeReadOp::GetSymbol,
        name: "get_symbol",
        summary: "Read exact symbol source bodies (single path+name or batch targets=[...]).",
        discovery_only: false,
        params: &[
            ParamSpec {
                name: "path",
                py_type: "str",
                required: false,
                default_repr: "\"\"",
                doc: "Relative file path when reading one symbol.",
            },
            ParamSpec {
                name: "name",
                py_type: "str",
                required: false,
                default_repr: "\"\"",
                doc: "Symbol name when reading one symbol.",
            },
            ParamSpec {
                name: "kind",
                py_type: "str | None",
                required: false,
                default_repr: "None",
                doc: "Disambiguating symbol kind.",
            },
            ParamSpec {
                name: "symbol_line",
                py_type: "int | None",
                required: false,
                default_repr: "None",
                doc: "1-based start line to disambiguate.",
            },
            ParamSpec {
                name: "targets",
                py_type: "list[dict] | None",
                required: false,
                default_repr: "None",
                doc: "Batch list of {'path': str, 'name': str, 'kind'?: str, 'symbol_line'?: int}.",
            },
            ParamSpec {
                name: "max_tokens",
                py_type: "int | None",
                required: false,
                default_repr: "None",
                doc: "Positive approximate token budget per source; clipped rows set truncated=True.",
            },
        ],
        return_type: "list[SymbolBody]",
        typed_dicts: "class SymbolBody(TypedDict):\n    path: str\n    name: str\n    kind: str\n    symbol_line: NotRequired[int]  # 1-based selector for a resolved named symbol; absent for slices and refusals.\n    start_line: int  # First line of returned source, including attached docs; not the symbol_line selector.\n    end_line: int  # Last line returned (inclusive).\n    source: str | None\n    truncated: NotRequired[bool]\n    refusal: str | None\n    withheld_reason: str | None",
        keywords: &[
            "symbol",
            "body",
            "source",
            "read",
            "batch",
            "targets",
            "definition",
            "code",
        ],
    },
    CodeOpDescriptor {
        op: CodeReadOp::FindReferences,
        name: "find_references",
        summary: "Find call sites, imports, type usages, or trait implementations for a symbol.",
        discovery_only: false,
        params: &[
            ParamSpec {
                name: "name",
                py_type: "str",
                required: true,
                default_repr: "",
                doc: "Symbol or trait name.",
            },
            ParamSpec {
                name: "kind",
                py_type: "str | None",
                required: false,
                default_repr: "None",
                doc: "'call', 'import', 'type_usage', or 'all'.",
            },
            ParamSpec {
                name: "path",
                py_type: "str | None",
                required: false,
                default_repr: "None",
                doc: "Target symbol's defining file path for disambiguation.",
            },
            ParamSpec {
                name: "symbol_kind",
                py_type: "str | None",
                required: false,
                default_repr: "None",
                doc: "Target symbol's kind.",
            },
            ParamSpec {
                name: "symbol_line",
                py_type: "int | None",
                required: false,
                default_repr: "None",
                doc: "Target symbol's 1-based line.",
            },
            ParamSpec {
                name: "mode",
                py_type: "str | None",
                required: false,
                default_repr: "None",
                doc: "'references' (default) or 'implementations'.",
            },
            ParamSpec {
                name: "limit",
                py_type: "int | None",
                required: false,
                default_repr: "None",
                doc: "Max files/entries.",
            },
            ParamSpec {
                name: "max_per_file",
                py_type: "int | None",
                required: false,
                default_repr: "None",
                doc: "Max reference rows per file (default 10, capped at 50); references mode only.",
            },
            ParamSpec {
                name: "context",
                py_type: "int",
                required: false,
                default_repr: "0",
                doc: "Lines (0-20) around each hit, numbered, in the row's `context`.",
            },
        ],
        return_type: "list[RefHit]",
        typed_dicts: "class RefHit(TypedDict):\n    path: str\n    line: int\n    text: str\n    enclosing_symbol: str | None\n    enclosing_line: int | None\n    caller_kind: str | None\n    context: NotRequired[str]",
        keywords: &[
            "references",
            "callers",
            "usages",
            "usages",
            "implementations",
            "trait",
            "calls",
            "xref",
        ],
    },
    CodeOpDescriptor {
        op: CodeReadOp::FindDependents,
        name: "find_dependents",
        summary: "Find files that import or depend on a given file path.",
        discovery_only: false,
        params: &[
            ParamSpec {
                name: "path",
                py_type: "str",
                required: true,
                default_repr: "",
                doc: "Relative file path whose dependents to list.",
            },
            ParamSpec {
                name: "name",
                py_type: "str | None",
                required: false,
                default_repr: "None",
                doc: "Optional imported symbol filter.",
            },
            ParamSpec {
                name: "limit",
                py_type: "int | None",
                required: false,
                default_repr: "None",
                doc: "Max dependent files.",
            },
            ParamSpec {
                name: "max_per_file",
                py_type: "int | None",
                required: false,
                default_repr: "None",
                doc: "Max matching dependent rows per file (default 10, capped at 50).",
            },
        ],
        return_type: "list[DependentHit]",
        typed_dicts: "class DependentHit(TypedDict):\n    path: str\n    line: int\n    kind: str\n    name: str\n    text: str",
        keywords: &[
            "dependents",
            "imports",
            "consumers",
            "reverse",
            "file",
            "breaks",
        ],
    },
    CodeOpDescriptor {
        op: CodeReadOp::FileContext,
        name: "file_context",
        summary: "Summarize a file's outline, imports, and consumers.",
        discovery_only: false,
        params: &[
            ParamSpec {
                name: "path",
                py_type: "str",
                required: true,
                default_repr: "",
                doc: "Relative file path.",
            },
            ParamSpec {
                name: "sections",
                py_type: "list[str] | None",
                required: false,
                default_repr: "None",
                doc: "Subset of ['outline', 'imports', 'consumers'].",
            },
            ParamSpec {
                name: "include_tests",
                py_type: "bool",
                required: false,
                default_repr: "False",
                doc: "Expand test modules.",
            },
            ParamSpec {
                name: "max_tokens",
                py_type: "int | None",
                required: false,
                default_repr: "None",
                doc: "Optional token budget.",
            },
        ],
        return_type: "FileContextOut",
        typed_dicts: "class FileContextOut(TypedDict):\n    path: str\n    rendered: str\n    selected_sections: list[str]\n    estimated_tokens: int | None\n    raw_file_tokens: int | None",
        keywords: &[
            "outline",
            "file",
            "context",
            "summary",
            "imports",
            "structure",
        ],
    },
    CodeOpDescriptor {
        op: CodeReadOp::FileContent,
        name: "file_content",
        summary: "Read exact UTF-8 source lines or slices from a file (secret-gated).",
        discovery_only: false,
        params: &[
            ParamSpec {
                name: "path",
                py_type: "str",
                required: true,
                default_repr: "",
                doc: "Relative file path.",
            },
            ParamSpec {
                name: "start_line",
                py_type: "int | None",
                required: false,
                default_repr: "None",
                doc: "1-based inclusive start line.",
            },
            ParamSpec {
                name: "end_line",
                py_type: "int | None",
                required: false,
                default_repr: "None",
                doc: "1-based inclusive end line.",
            },
            ParamSpec {
                name: "around_symbol",
                py_type: "str | None",
                required: false,
                default_repr: "None",
                doc: "Center read window on named symbol.",
            },
            ParamSpec {
                name: "show_line_numbers",
                py_type: "bool | None",
                required: false,
                default_repr: "None",
                doc: "Prefix lines with line numbers.",
            },
            ParamSpec {
                name: "max_tokens",
                py_type: "int | None",
                required: false,
                default_repr: "None",
                doc: "Optional token budget.",
            },
        ],
        return_type: "FileContentOut",
        typed_dicts: "class FileContentOut(TypedDict):\n    path: str\n    content_hash: str\n    content: str\n    approximate_tokens: int",
        keywords: &["read", "file", "content", "lines", "raw", "bytes", "source"],
    },
    CodeOpDescriptor {
        op: CodeReadOp::SearchText,
        name: "search_text",
        summary: "Search code text via literal, regex, or structural AST pattern (does not bump frecency).",
        discovery_only: true,
        params: &[
            ParamSpec {
                name: "query",
                py_type: "str",
                required: true,
                default_repr: "",
                doc: "Literal text, regex, or AST pattern.",
            },
            ParamSpec {
                name: "regex",
                py_type: "bool | None",
                required: false,
                default_repr: "None",
                doc: "Treat query as regex.",
            },
            ParamSpec {
                name: "structural",
                py_type: "bool | None",
                required: false,
                default_repr: "None",
                doc: "Treat query as ast-grep structural pattern.",
            },
            ParamSpec {
                name: "path_prefix",
                py_type: "str | None",
                required: false,
                default_repr: "None",
                doc: "Restrict to directory/file prefix.",
            },
            ParamSpec {
                name: "language",
                py_type: "str | None",
                required: false,
                default_repr: "None",
                doc: "Language filter.",
            },
            ParamSpec {
                name: "include_tests",
                py_type: "bool | None",
                required: false,
                default_repr: "None",
                doc: "Include test files (default False).",
            },
            ParamSpec {
                name: "case_sensitive",
                py_type: "bool | None",
                required: false,
                default_repr: "None",
                doc: "Case-sensitive matching.",
            },
            ParamSpec {
                name: "group_by",
                py_type: "str | None",
                required: false,
                default_repr: "None",
                doc: "Pass 'count' for exact per-file match counts.",
            },
            ParamSpec {
                name: "limit",
                py_type: "int | None",
                required: false,
                default_repr: "None",
                doc: "Max matches (or max files when group_by='count').",
            },
            ParamSpec {
                name: "max_per_file",
                py_type: "int | None",
                required: false,
                default_repr: "None",
                doc: "Max match rows per file (default 5); ignored when group_by='count'.",
            },
            ParamSpec {
                name: "context",
                py_type: "int",
                required: false,
                default_repr: "0",
                doc: "Lines (0-20) around each hit, numbered, in the row's `context`.",
            },
        ],
        return_type: "list[TextHit] | list[TextCountHit]",
        typed_dicts: "class TextHit(TypedDict):\n    path: str\n    line: int\n    preview: str\n    enclosing_symbol: str | None\n    context: NotRequired[str]\n\nclass TextCountHit(TypedDict):\n    path: str\n    count: int",
        keywords: &[
            "grep", "text", "search", "regex", "pattern", "ast", "literal", "code",
        ],
    },
    CodeOpDescriptor {
        op: CodeReadOp::SearchFiles,
        name: "search_files",
        summary: "Locate repository files by path or filename (does not bump frecency).",
        discovery_only: true,
        params: &[
            ParamSpec {
                name: "query",
                py_type: "str",
                required: true,
                default_repr: "",
                doc: "Filename or path query.",
            },
            ParamSpec {
                name: "path_prefix",
                py_type: "str | None",
                required: false,
                default_repr: "None",
                doc: "Restrict under prefix.",
            },
            ParamSpec {
                name: "limit",
                py_type: "int | None",
                required: false,
                default_repr: "None",
                doc: "Max files.",
            },
            ParamSpec {
                name: "resolve",
                py_type: "bool | None",
                required: false,
                default_repr: "None",
                doc: "Resolve best single path.",
            },
        ],
        return_type: "list[FileHit]",
        typed_dicts: "class FileHit(TypedDict):\n    path: str\n    tier: str\n    coupling_score: float | None",
        keywords: &["files", "path", "filename", "glob", "locate", "find"],
    },
    CodeOpDescriptor {
        op: CodeReadOp::RepoMap,
        name: "repo_map",
        summary: "Orient in repository directory and symbol layout.",
        discovery_only: true,
        params: &[
            ParamSpec {
                name: "path",
                py_type: "str | None",
                required: false,
                default_repr: "None",
                doc: "Subdirectory scope.",
            },
            ParamSpec {
                name: "detail",
                py_type: "str | None",
                required: false,
                default_repr: "None",
                doc: "'compact', 'tree', or 'full'.",
            },
            ParamSpec {
                name: "depth",
                py_type: "int | None",
                required: false,
                default_repr: "None",
                doc: "Directory traversal depth.",
            },
            ParamSpec {
                name: "max_files",
                py_type: "int | None",
                required: false,
                default_repr: "None",
                doc: "Max files shown.",
            },
        ],
        return_type: "RepoMapOut",
        typed_dicts: "class RepoMapOut(TypedDict):\n    detail: str\n    rendered: str\n    total_files: int\n    shown_files: int",
        keywords: &["repo", "map", "tree", "layout", "directory", "overview"],
    },
    CodeOpDescriptor {
        op: CodeReadOp::SymbolContext,
        name: "symbol_context",
        summary: "Trace a symbol's body, callers, callees, type usages, and referenced type definitions.",
        discovery_only: false,
        params: &[
            ParamSpec {
                name: "name",
                py_type: "str",
                required: true,
                default_repr: "",
                doc: "Symbol name.",
            },
            ParamSpec {
                name: "path",
                py_type: "str | None",
                required: false,
                default_repr: "None",
                doc: "Disambiguating file path.",
            },
            ParamSpec {
                name: "symbol_kind",
                py_type: "str | None",
                required: false,
                default_repr: "None",
                doc: "Symbol kind filter.",
            },
            ParamSpec {
                name: "symbol_line",
                py_type: "int | None",
                required: false,
                default_repr: "None",
                doc: "1-based start line.",
            },
            ParamSpec {
                name: "bundle",
                py_type: "bool | None",
                required: false,
                default_repr: "None",
                doc: "Inline referenced type bodies.",
            },
            ParamSpec {
                name: "include_tests",
                py_type: "bool",
                required: false,
                default_repr: "False",
                doc: "Include test callers.",
            },
        ],
        return_type: "SymbolContextOut",
        typed_dicts: "class CallerRef(TypedDict):\n    name: str\n    path: str\n    caller_kind: str | None\n    caller_line: int | None\n    call_line: int\n    call_expr: str\n    confidence: str\n    occurrences: int\n\nclass SymbolContextOut(TypedDict):\n    path: str | None\n    name: str | None\n    kind: str | None\n    body: str | None\n    total_callers: int\n    callers: list[CallerRef]\n    callees: list[dict]\n    type_usages: list[dict]\n    dependencies: list[dict]",
        keywords: &[
            "context", "trace", "callers", "callees", "bundle", "types", "graph",
        ],
    },
    CodeOpDescriptor {
        op: CodeReadOp::InspectMatch,
        name: "inspect_match",
        summary: "Inspect a path+line match with enclosing symbol, parent chain, and siblings.",
        discovery_only: false,
        params: &[
            ParamSpec {
                name: "path",
                py_type: "str",
                required: true,
                default_repr: "",
                doc: "Relative file path.",
            },
            ParamSpec {
                name: "line",
                py_type: "int",
                required: true,
                default_repr: "",
                doc: "1-based line number.",
            },
            ParamSpec {
                name: "context",
                py_type: "int | None",
                required: false,
                default_repr: "None",
                doc: "Surrounding context lines.",
            },
        ],
        return_type: "InspectMatchOut",
        typed_dicts: "class InspectMatchOut(TypedDict):\n    path: str\n    line: int\n    excerpt: str\n    enclosing: dict | None\n    parent_chain: list[dict]\n    siblings: list[dict]",
        keywords: &[
            "inspect",
            "match",
            "line",
            "enclosing",
            "siblings",
            "parent",
        ],
    },
    CodeOpDescriptor {
        op: CodeReadOp::WhatChanged,
        name: "what_changed",
        summary: "List uncommitted, git-ref, or timestamp-filtered changed files and optional symbol deltas.",
        discovery_only: true,
        params: &[
            ParamSpec {
                name: "git_ref",
                py_type: "str | None",
                required: false,
                default_repr: "None",
                doc: "Git ref to compare against.",
            },
            ParamSpec {
                name: "uncommitted",
                py_type: "bool | None",
                required: false,
                default_repr: "None",
                doc: "Inspect uncommitted working tree changes.",
            },
            ParamSpec {
                name: "since",
                py_type: "int | None",
                required: false,
                default_repr: "None",
                doc: "Unix epoch seconds.",
            },
            ParamSpec {
                name: "path_prefix",
                py_type: "str | None",
                required: false,
                default_repr: "None",
                doc: "Path prefix filter.",
            },
            ParamSpec {
                name: "include_symbol_diff",
                py_type: "bool | None",
                required: false,
                default_repr: "None",
                doc: "Include per-file symbol diff.",
            },
        ],
        return_type: "WhatChangedOut",
        typed_dicts: "class WhatChangedOut(TypedDict):\n    paths: list[str]\n    total_paths: int\n    symbol_diff: dict | None",
        keywords: &[
            "changed",
            "git",
            "status",
            "uncommitted",
            "modified",
            "files",
            "diff",
        ],
    },
    CodeOpDescriptor {
        op: CodeReadOp::DiffSymbols,
        name: "diff_symbols",
        summary: "Compare added, removed, and modified symbols between git refs or working tree.",
        discovery_only: true,
        params: &[
            ParamSpec {
                name: "base",
                py_type: "str | None",
                required: false,
                default_repr: "None",
                doc: "Base git ref (default HEAD).",
            },
            ParamSpec {
                name: "target",
                py_type: "str | None",
                required: false,
                default_repr: "None",
                doc: "Target git ref (omit for working tree).",
            },
            ParamSpec {
                name: "path_prefix",
                py_type: "str | None",
                required: false,
                default_repr: "None",
                doc: "Path prefix filter.",
            },
        ],
        return_type: "DiffSymbolsOut",
        typed_dicts: "class DiffSymbolsOut(TypedDict):\n    base: str\n    target: str\n    added_count: int\n    removed_count: int\n    modified_count: int\n    withheld_count: int\n    files: list[dict]",
        keywords: &[
            "diff", "symbols", "git", "added", "removed", "modified", "commit",
        ],
    },
    CodeOpDescriptor {
        op: CodeReadOp::DetectImpact,
        name: "detect_impact",
        summary: "Compute call-graph blast radius from git and working-tree symbol changes.",
        discovery_only: true,
        params: &[
            ParamSpec {
                name: "base_branch",
                py_type: "str | None",
                required: false,
                default_repr: "None",
                doc: "Base git ref/branch.",
            },
            ParamSpec {
                name: "depth",
                py_type: "int | None",
                required: false,
                default_repr: "None",
                doc: "Call-graph hop depth (default 2).",
            },
            ParamSpec {
                name: "paths",
                py_type: "list[str] | None",
                required: false,
                default_repr: "None",
                doc: "Explicit seed file paths.",
            },
        ],
        return_type: "DetectImpactOut",
        typed_dicts: "class DetectImpactOut(TypedDict):\n    changed_files: list[str]\n    changed_symbols: list[dict]\n    blast_radius: list[dict]",
        keywords: &[
            "impact",
            "blast",
            "radius",
            "risk",
            "git",
            "hops",
            "dependents",
        ],
    },
    CodeOpDescriptor {
        op: CodeReadOp::SearchKnowledge,
        name: "search_knowledge",
        summary: "Search admitted Markdown/documentation corpus with authority metadata.",
        discovery_only: true,
        params: &[
            ParamSpec {
                name: "query",
                py_type: "str",
                required: true,
                default_repr: "",
                doc: "Documentation search query.",
            },
            ParamSpec {
                name: "path_prefix",
                py_type: "str | None",
                required: false,
                default_repr: "None",
                doc: "Doc path prefix.",
            },
            ParamSpec {
                name: "authority_scope",
                py_type: "str | None",
                required: false,
                default_repr: "None",
                doc: "'default', 'current', 'intent', 'history', or 'all'.",
            },
            ParamSpec {
                name: "limit",
                py_type: "int | None",
                required: false,
                default_repr: "None",
                doc: "Max doc hits.",
            },
            ParamSpec {
                name: "include_archived",
                py_type: "bool | None",
                required: false,
                default_repr: "None",
                doc: "Include archived/dated review docs.",
            },
        ],
        return_type: "SearchKnowledgeOut",
        typed_dicts: "class SearchKnowledgeOut(TypedDict):\n    rendered: str\n    hit_count: int\n    rendered_hit_count: int\n    overflow: int\n    withheld_count: int",
        keywords: &[
            "knowledge",
            "docs",
            "markdown",
            "spec",
            "adr",
            "documentation",
            "readme",
        ],
    },
];

fn render_signature_line(desc: &CodeOpDescriptor) -> String {
    let mut params = String::new();
    for (idx, p) in desc.params.iter().enumerate() {
        if idx > 0 {
            params.push_str(", ");
        }
        if p.required {
            params.push_str(p.name);
            params.push_str(": ");
            params.push_str(p.py_type);
        } else {
            params.push_str(p.name);
            params.push_str(": ");
            params.push_str(p.py_type);
            params.push_str(" = ");
            params.push_str(p.default_repr);
        }
    }
    if params.is_empty() {
        format!("def {}() -> {}", desc.name, desc.return_type)
    } else {
        format!("def {}(*, {}) -> {}", desc.name, params, desc.return_type)
    }
}

fn render_full_stub(desc: &CodeOpDescriptor) -> String {
    let mut out = String::new();
    out.push_str(desc.typed_dicts);
    out.push_str("\n\n");
    out.push_str(&render_signature_line(desc));
    out.push_str(":\n    \"\"\"");
    out.push_str(desc.summary);
    out.push('\n');
    for p in desc.params {
        out.push_str("    - ");
        out.push_str(p.name);
        out.push_str(": ");
        out.push_str(p.doc);
        out.push('\n');
    }
    out.push_str("    \"\"\"\n    ...");
    out
}

/// Renders Python stubs for `code_search` and `symforge code stubs`.
///
/// - When `query` is `None` or blank: returns a compact 1-line signature index of all 15 functions.
/// - When `query` is non-empty: ranks matching functions by name/keyword/summary match and renders
///   their full `TypedDict` return definitions + keyword-only `def` stubs.
#[must_use]
pub fn render_stubs(query: Option<&str>, limit: Option<usize>) -> String {
    let trimmed = query.map(str::trim).unwrap_or("");
    if trimmed.is_empty() {
        let mut out = String::from(
            "# SymForge Code Mode — keyword-only Python functions (pass query to inspect TypedDict return fields)\n",
        );
        for desc in CODE_READ_OPS {
            out.push_str(&render_signature_line(desc));
            out.push('\n');
        }
        return out;
    }

    let terms: Vec<String> = trimmed
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .filter(|t| !t.is_empty())
        .map(|t| t.to_ascii_lowercase())
        .collect();

    let mut scored: Vec<(u32, &CodeOpDescriptor)> = CODE_READ_OPS
        .iter()
        .filter_map(|desc| {
            let mut score = 0u32;
            let name_lower = desc.name.to_ascii_lowercase();
            let summary_lower = desc.summary.to_ascii_lowercase();
            for term in &terms {
                if name_lower == *term {
                    score += 100;
                } else if name_lower.contains(term) {
                    score += 50;
                }
                if desc.keywords.iter().any(|k| *k == term) {
                    score += 35;
                } else if desc.keywords.iter().any(|k| k.contains(term.as_str())) {
                    score += 15;
                }
                if summary_lower.contains(term) {
                    score += 10;
                }
                if desc
                    .params
                    .iter()
                    .any(|p| p.name.eq_ignore_ascii_case(term))
                {
                    score += 10;
                }
            }
            (score > 0).then_some((score, desc))
        })
        .collect();

    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(b.1.name)));
    let cap = limit.unwrap_or(5).clamp(1, CODE_READ_OPS.len());

    if scored.is_empty() {
        let names: Vec<&str> = CODE_READ_OPS.iter().map(|d| d.name).collect();
        return format!(
            "# No Code Mode stub matched {trimmed:?}. Available functions: {}\n# Pass an empty query to list all one-line signatures.\n",
            names.join(", ")
        );
    }

    let mut out = String::from("from typing import NotRequired, TypedDict\n\n");
    for (idx, (_, desc)) in scored.into_iter().take(cap).enumerate() {
        if idx > 0 {
            out.push_str("\n\n");
        }
        out.push_str(&render_full_stub(desc));
    }
    out.push('\n');
    out
}

/// Renders the self-contained agent manual for `symforge code idioms`.
#[must_use]
pub fn render_idioms() -> String {
    let mut fn_list = String::new();
    for desc in CODE_READ_OPS {
        fn_list.push_str("- `");
        fn_list.push_str(&render_signature_line(desc));
        fn_list.push_str("` — ");
        fn_list.push_str(desc.summary);
        fn_list.push('\n');
    }

    format!(
        "# SymForge Code Mode Idioms (Pure-Rust Monty Python 3.14 Subset)\n\
\n\
## When to use which lane\n\
- **Single query (quote-free, PowerShell/cmd/bash safe)**: `symforge q <op> -a key=val -a limit=5` (also accepts `symforge q <op> '<json>'` or `--args-file args.json`; zero script overhead, pure JSON stdout).\n\
- **Multi-step chain / filtering**: `symforge code run script.py`, PowerShell `@' ... '@ | symforge code run -`, or `symforge code run -c \"<python>\"`.\n\
- **Inspect return fields**: `symforge code stubs <topic>` (e.g. `symforge code stubs references`).\n\
- **Known source locations**: Return targeted `file_content` windows. Use `symbol_context(bundle=True, ...)` when you also need caller, callee, or type evidence.\n\
- **Text search**: Use `group_by=\"count\"` only when locations are unnecessary. When returning many hits, group by path in the script to avoid repeating paths.\n\
- **Partial answers**: Check host-call completeness notices even when the script returns a scalar or empty list. Omitted rows are known matches. `withheld_files` counts files in the call's search scope that were not searched because they are withheld (policy or size); it repeats across calls with the same scope and is not a count of files with more hits, since those files may or may not contain matches.\n\
- **Edits**: Mutations NEVER run inside Code Mode; perform edits explicitly after narrowing.\n\
\n\
## Sandbox rules & resource caps\n\
1. **Keyword arguments ONLY**: Every host function requires `fn(key=value)` — positional arguments raise `TypeError`.\n\
2. **Final expression = JSON `value`**: The last top-level expression of the script becomes `\"value\"` in stdout JSON (`print(...)` goes to `\"stdout\"`).\n\
3. **Pure-Rust Monty subset**: List/dict comprehensions, `for`/`if`/`while`, `def`, slicing, `json`, `re`, `math` work normally. Avoid `yield` generators, `del`, `open()`, `os.*`, network I/O, or third-party imports (zero-egress sandbox raises `PermissionError` / `ModuleNotFoundError`).\n\
4. **Bounds**: 10s feed CPU/wall budget (30s ceiling), 64 MiB heap (256 MiB ceiling), 150 recursion depth, 64 host calls, 32,000 inline chars (larger outputs write a lossless JSON spill file reported in `spill_path`).\n\
\n\
## Registered read-only functions ({count})\n\
{fn_list}\n\
## Worked example: find non-test callers and read their bodies in 1 turn\n\
```python\n{example}\n```\n",
        count = CODE_READ_OPS.len(),
        example = r#"hits = search_symbols(query="enforce_compact_surface", kind="fn")
refs = find_references(name="enforce_compact_surface", kind="call")
caller_targets = []
seen = set()
for r in refs:
    caller = r.get("enclosing_symbol")
    line = r.get("enclosing_line")
    kind = r.get("caller_kind")
    key = (r["path"], caller, kind, line)
    if caller and line and not r["path"].startswith("tests/") and key not in seen:
        seen.add(key)
        caller_targets.append({"path": r["path"], "name": caller, "kind": kind, "symbol_line": line})
bodies = get_symbol(targets=caller_targets[:5]) if caller_targets else []
{"definitions": hits, "visible_caller_count": len(caller_targets), "caller_bodies": bodies}"#,
    )
}
