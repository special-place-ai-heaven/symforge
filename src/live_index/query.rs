use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use crate::domain::{LanguageId, ReferenceKind, ReferenceRecord, SymbolRecord};

pub use super::context_bundle::{
    ContextBundleFoundView, ContextBundleReferenceView, ContextBundleSectionView,
    ContextBundleView, ImplBlockSuggestionView, TypeDependencyView,
};
#[cfg(test)]
use super::disambiguation::kind_disambiguation_tier;
pub(crate) use super::disambiguation::{
    SymbolSelectorMatch, render_symbol_selector, resolve_symbol_selector,
};
use super::disambiguation::{
    is_receiver_method_call, matches_exact_symbol_qualified_name, matches_exact_symbol_reference,
    parse_reference_kind_filter,
};
pub use super::health_view::{
    AdmissionTierLookupView, EXPECTED_VENDOR_PARTIAL_PARSE_REASON, HealthStats,
};
use super::search::{NoiseClass, NoisePolicy, PathScope};
use super::store::{IndexedFile, LiveIndex};

// ---------------------------------------------------------------------------
// Module path resolution for find_dependents
// ---------------------------------------------------------------------------

/// Resolve the logical module path for a file based on language conventions.
///
/// Returns `None` if the file doesn't follow a recognized module convention.
///
/// Examples (Rust):
///   "src/lib.rs"              → Some("crate")
///   "src/main.rs"             → Some("crate")
///   "src/error.rs"            → Some("crate::error")
///   "src/live_index/mod.rs"   → Some("crate::live_index")
///   "src/live_index/store.rs" → Some("crate::live_index::store")
///
/// Examples (Python):
///   "src/__init__.py"         → Some("src")
///   "src/foo.py"              → Some("src.foo")
///   "src/foo/__init__.py"     → Some("src.foo")
///
/// Examples (JS/TS):
///   "src/index.js"            → Some("src")
///   "src/utils/index.ts"      → Some("src/utils")
fn resolve_module_path(file_path: &str, language: &LanguageId) -> Option<String> {
    let path = std::path::Path::new(file_path);

    match language {
        LanguageId::Rust => {
            // Strip up to and including "src/" — handles both root projects ("src/lib.rs")
            // and workspace crates ("crates/aap-core/src/types.rs").
            let after_src: String = if let Ok(stripped) = path.strip_prefix("src") {
                stripped.to_string_lossy().into_owned()
            } else {
                // Workspace layout: find "/src/" component anywhere in path
                let normalized = file_path.replace('\\', "/");
                let src_idx = normalized.find("/src/")?;
                normalized[src_idx + 5..].to_string() // skip "/src/"
            };
            let stripped = std::path::Path::new(&after_src);
            let mut components: Vec<String> = stripped
                .components()
                .filter_map(|c| c.as_os_str().to_str().map(String::from))
                .collect();

            // Remove extension from last component
            if let Some(last) = components.last_mut()
                && let Some(stem) = std::path::Path::new(last.as_str())
                    .file_stem()
                    .and_then(|s| s.to_str())
            {
                *last = stem.to_string();
            }

            // Drop "lib", "main", "mod" — these map to their parent module
            if matches!(
                components.last().map(|s| s.as_str()),
                Some("lib" | "main" | "mod")
            ) {
                components.pop();
            }

            if components.is_empty() {
                Some("crate".to_string())
            } else {
                Some(format!("crate::{}", components.join("::")))
            }
        }
        LanguageId::Python => {
            let mut components: Vec<String> = path
                .components()
                .filter_map(|c| c.as_os_str().to_str().map(String::from))
                .collect();

            // Remove extension from last component
            if let Some(last) = components.last_mut()
                && let Some(stem) = std::path::Path::new(last.as_str())
                    .file_stem()
                    .and_then(|s| s.to_str())
            {
                *last = stem.to_string();
            }

            // Drop __init__ — maps to the package (parent directory)
            if matches!(components.last().map(|s| s.as_str()), Some("__init__")) {
                components.pop();
            }

            if components.is_empty() {
                None
            } else {
                Some(components.join("."))
            }
        }
        LanguageId::JavaScript | LanguageId::TypeScript => {
            let mut components: Vec<String> = path
                .components()
                .filter_map(|c| c.as_os_str().to_str().map(String::from))
                .collect();

            // Remove extension from last component
            if let Some(last) = components.last_mut()
                && let Some(stem) = std::path::Path::new(last.as_str())
                    .file_stem()
                    .and_then(|s| s.to_str())
            {
                *last = stem.to_string();
            }

            // Drop "index" — maps to the directory
            if matches!(components.last().map(|s| s.as_str()), Some("index")) {
                components.pop();
            }

            if components.is_empty() {
                None
            } else {
                Some(components.join("/"))
            }
        }
        _ => None,
    }
}

/// Check if an import reference in a file is a `pub use` (re-export).
///
/// Looks at the source content just before the import's byte range to find a `pub` keyword.
fn is_pub_use_import(file: &IndexedFile, reference: &ReferenceRecord) -> bool {
    if reference.kind != ReferenceKind::Import {
        return false;
    }
    // Look back from the import start to find `pub use` or `pub(crate) use`, etc.
    let start = reference.byte_range.0 as usize;
    // Grab up to 30 bytes before the reference start (enough for `pub(crate) use `)
    let lookback_start = start.saturating_sub(30);
    if lookback_start >= file.content.len() || start > file.content.len() {
        return false;
    }
    let prefix = &file.content[lookback_start..start];
    // Lossy decode: a fixed 30-byte lookback can split a multibyte UTF-8 char.
    // `from_utf8(..).unwrap_or("")` would blank the whole prefix and silently
    // report a genuine `pub use` re-export as non-public; lossy decode keeps the
    // `pub`-prefix detection intact (a split codepoint becomes U+FFFD).
    let prefix_str = String::from_utf8_lossy(prefix);
    // Check if the line containing this import starts with `pub`
    let line_start = prefix_str.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_prefix = prefix_str[line_start..].trim_start();
    line_prefix.starts_with("pub ") || line_prefix.starts_with("pub(") || line_prefix == "pub"
}

/// Find files that re-export symbols from `target_module_path` via `pub use`.
///
/// Returns the file paths of re-exporter files.
fn find_reexporters<'a>(
    files: &'a std::collections::HashMap<String, Arc<IndexedFile>>,
    target_path: &str,
    target_module_path: Option<&str>,
    target_language: &LanguageId,
    target_stem: &str,
) -> Vec<&'a str> {
    if *target_language != LanguageId::Rust {
        return vec![];
    }

    let mut reexporters = Vec::new();
    for (file_path, file) in files {
        if file_path.as_str() == target_path {
            continue;
        }
        if file.language != *target_language {
            continue;
        }
        for reference in &file.references {
            if matches_target_import(
                &file.language,
                target_language,
                reference,
                target_stem,
                target_module_path,
            ) && is_pub_use_import(file, reference)
            {
                reexporters.push(file_path.as_str());
                break;
            }
        }
    }
    reexporters
}

fn declared_scope(file: &IndexedFile) -> Option<String> {
    let content = String::from_utf8_lossy(&file.content);
    match file.language {
        LanguageId::CSharp => parse_declared_scope(&content, "namespace"),
        LanguageId::Java => parse_declared_scope(&content, "package"),
        _ => None,
    }
}

fn parse_declared_scope(content: &str, keyword: &str) -> Option<String> {
    content.lines().find_map(|line| {
        let trimmed = line.split("//").next().unwrap_or("").trim();
        let rest = trimmed.strip_prefix(keyword)?.trim_start();
        let scope: String = rest
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '.')
            .collect();
        if scope.is_empty() { None } else { Some(scope) }
    })
}

fn imported_scope(language: &LanguageId, reference: &ReferenceRecord) -> Option<String> {
    if reference.kind != ReferenceKind::Import {
        return None;
    }

    let qualified_name = reference.qualified_name.as_deref()?;
    match language {
        LanguageId::CSharp => Some(qualified_name.to_string()),
        LanguageId::Java => {
            let trimmed = qualified_name.trim_end_matches(".*");
            match trimmed.rsplit_once('.') {
                Some((scope, _)) if !scope.is_empty() => Some(scope.to_string()),
                _ if !trimmed.is_empty() => Some(trimmed.to_string()),
                _ => None,
            }
        }
        _ => None,
    }
}

fn can_match_type_dependents(
    dependent_file: &IndexedFile,
    target_language: &LanguageId,
    target_scope: Option<&str>,
) -> bool {
    if &dependent_file.language != target_language {
        return false;
    }

    match target_language {
        LanguageId::CSharp | LanguageId::Java => {
            let Some(target_scope) = target_scope else {
                return true;
            };

            if declared_scope(dependent_file).as_deref() == Some(target_scope) {
                return true;
            }

            dependent_file
                .references
                .iter()
                .filter_map(|reference| imported_scope(&dependent_file.language, reference))
                .any(|scope| scope == target_scope)
        }
        _ => false,
    }
}

fn matches_target_stem(text: &str, stem: &str) -> bool {
    text == stem
        || text.ends_with(&format!("/{stem}"))
        || text.ends_with(&format!("::{stem}"))
        || text.ends_with(&format!(".{stem}"))
        || text.contains(&format!("/{stem}/"))
        || text.contains(&format!("::{stem}::"))
        // Relative imports: `index::Thing` matches stem "index"
        || text.starts_with(&format!("{stem}::"))
        || text.starts_with(&format!("{stem}/"))
        || text.starts_with(&format!("{stem}."))
}

fn matches_target_module(language: &LanguageId, text: &str, module_path: Option<&str>) -> bool {
    let Some(module_path) = module_path else {
        return false;
    };

    let module_sep = match language {
        LanguageId::Python => ".",
        LanguageId::JavaScript | LanguageId::TypeScript => "/",
        _ => "::",
    };

    if text == module_path || text.starts_with(&format!("{module_path}{module_sep}")) {
        return true;
    }

    if *language == LanguageId::Rust
        && let Some(tail) = module_path.strip_prefix("crate::")
    {
        return text == tail
            || text.ends_with(&format!("::{tail}"))
            || text.contains(&format!("::{tail}::"));
    }

    false
}

/// Returns `true` when an import written in `importer_language` could plausibly
/// resolve to a file written in `target_language`.
///
/// Import resolution is language-scoped: a Python `import gguf` resolves within
/// the Python module namespace, never to a Rust `gguf.rs`; a Rust `use gguf`
/// resolves within the crate, never to a Python `gguf.py`. Bare module names are
/// frequently shared across unrelated languages in mixed monorepos (e.g. a Rust
/// `launcher/src/gguf.rs` alongside a Python `gguf` package), so matching imports
/// to a target file without checking language conflates them.
///
/// The only genuine cross-language interop for bare-module imports is the
/// JavaScript/TypeScript family, where a `.js` file may import a `.ts` module and
/// vice versa; those are treated as mutually compatible.
fn import_languages_compatible(
    importer_language: &LanguageId,
    target_language: &LanguageId,
) -> bool {
    if importer_language == target_language {
        return true;
    }

    matches!(
        (importer_language, target_language),
        (LanguageId::JavaScript, LanguageId::TypeScript)
            | (LanguageId::TypeScript, LanguageId::JavaScript)
    )
}

fn matches_target_import(
    importer_language: &LanguageId,
    target_language: &LanguageId,
    reference: &ReferenceRecord,
    stem: &str,
    module_path: Option<&str>,
) -> bool {
    if reference.kind != ReferenceKind::Import {
        return false;
    }

    // Import resolution is language-scoped. A bare module name shared across
    // unrelated languages (e.g. Python `import gguf` vs. Rust `gguf.rs`) must not
    // be treated as a dependency edge.
    if !import_languages_compatible(importer_language, target_language) {
        return false;
    }

    matches_target_stem(&reference.name, stem)
        || reference
            .qualified_name
            .as_deref()
            .map(|text| {
                matches_target_stem(text, stem)
                    || matches_target_module(target_language, text, module_path)
            })
            .unwrap_or(false)
        || matches_target_module(target_language, &reference.name, module_path)
}

// ---------------------------------------------------------------------------
// Built-in type filter lists (per-language)
// ---------------------------------------------------------------------------

const RUST_BUILTINS: &[&str] = &[
    "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize", "f32",
    "f64", "bool", "char", "str", "String", "Self", "self",
];

const PYTHON_BUILTINS: &[&str] = &[
    "int", "float", "str", "bool", "list", "dict", "tuple", "set", "None", "bytes", "object",
    "type",
];

const JS_BUILTINS: &[&str] = &[
    "string",
    "number",
    "boolean",
    "undefined",
    "null",
    "Object",
    "Array",
    "Function",
    "Symbol",
    "Promise",
    "Error",
];

const TS_BUILTINS: &[&str] = &[
    "string",
    "number",
    "boolean",
    "undefined",
    "null",
    "void",
    "never",
    "any",
    "unknown",
    "Object",
    "Array",
    "Function",
    "Symbol",
    "Promise",
    "Error",
    "Record",
    "Partial",
    "Required",
    "Readonly",
    "Pick",
    "Omit",
];

const GO_BUILTINS: &[&str] = &[
    "int",
    "int8",
    "int16",
    "int32",
    "int64",
    "uint",
    "uint8",
    "uint16",
    "uint32",
    "uint64",
    "float32",
    "float64",
    "complex64",
    "complex128",
    "bool",
    "string",
    "byte",
    "rune",
    "error",
    "any",
];

const JAVA_BUILTINS: &[&str] = &[
    "int",
    "long",
    "short",
    "byte",
    "float",
    "double",
    "boolean",
    "char",
    "void",
    "String",
    "Object",
    "Integer",
    "Long",
    "Short",
    "Byte",
    "Float",
    "Double",
    "Boolean",
    "Character",
];

/// Single-letter generic type parameter names that are almost always noise.
const SINGLE_LETTER_GENERICS: &[&str] = &[
    "T", "K", "V", "E", "R", "S", "A", "B", "C", "D", "N", "M", "P", "U", "W", "X", "Y", "Z",
];

/// Returns `true` when `name` is a known built-in primitive/stdlib type for
/// the file's language, or a single-letter generic parameter that would
/// generate false-positive matches across languages.
pub(super) fn is_filtered_name(name: &str, language: &LanguageId) -> bool {
    if SINGLE_LETTER_GENERICS.contains(&name) {
        return true;
    }

    let builtins = match language {
        LanguageId::Rust => RUST_BUILTINS,
        LanguageId::Python => PYTHON_BUILTINS,
        LanguageId::JavaScript => JS_BUILTINS,
        LanguageId::TypeScript => TS_BUILTINS,
        LanguageId::Go => GO_BUILTINS,
        LanguageId::Java => JAVA_BUILTINS,
        _ => &[],
    };

    builtins.contains(&name)
}

/// Returns true when `path` lives under a vendored / third-party directory.
/// Delegates to `NoisePolicy::classify_path` so the vendor-set stays
/// single-sourced with the indexer's noise classifier.
pub(crate) fn is_vendor_path(path: &str) -> bool {
    matches!(NoisePolicy::classify_path(path, None), NoiseClass::Vendor)
}

/// Returns true when `path` is personal-tooling sidecar content under
/// `.claude/gsd-*`, `.claude/get-shit-done/`, or an Obsidian `.obsidian/`
/// directory. Excludes shared agent infrastructure like `.claude/CLAUDE.md`,
/// `.claude/commands/`, `.claude/skills/`, `.claude/hooks/`, `.claude/agents/`.
pub(crate) fn is_personal_tooling_path(path: &str) -> bool {
    let lower = path.replace('\\', "/").to_ascii_lowercase();
    let has_obsidian_dir = lower
        .split('/')
        .filter(|segment| !segment.is_empty())
        .any(|segment| segment == ".obsidian");

    has_obsidian_dir
        || lower.starts_with(".claude/gsd-")
        || lower.starts_with(".claude/get-shit-done/")
}

pub(super) fn normalize_path_query(raw: &str) -> String {
    let mut normalized = raw.trim().replace('\\', "/");
    while normalized.starts_with("./") {
        normalized = normalized[2..].to_string();
    }
    normalized.trim_matches('/').to_string()
}

fn tokenize_path_query(normalized_query: &str) -> Vec<String> {
    normalized_query
        .split(|ch: char| ch == '/' || ch.is_whitespace())
        .filter(|part| !part.is_empty())
        .map(|part| part.to_ascii_lowercase())
        .collect()
}

/// Compute the [`PathMatchSignal`] tier score the given anchor path earns for
/// `query`, using the same normalization and tokenization `search_files` uses
/// for candidate classification. The co-change anchor-confidence gate compares
/// this score against `CO_CHANGE_ANCHOR_CONFIDENCE_FLOOR`; the caller reuses it
/// to report a precise fallback reason without re-tokenizing the query inline.
pub(crate) fn anchor_path_match_score(query: &str, anchor_path: &str) -> f32 {
    let normalized_query = normalize_path_query(query);
    let tokens = tokenize_path_query(&normalized_query);
    let ctx = super::rank_signals::RankCtx {
        query: &normalized_query,
        tokens: &tokens,
        current_file: None,
        target_path: None,
        co_change_count: None,
        co_change_weighted_score: None,
    };
    <super::rank_signals::PathMatchSignal as super::rank_signals::RankSignal>::score(
        &super::rank_signals::PathMatchSignal,
        std::path::Path::new(anchor_path),
        &ctx,
    )
}

pub(super) fn path_has_component(path: &str, component: &str) -> bool {
    path.split('/')
        .any(|part| part.eq_ignore_ascii_case(component))
}

fn shared_directory_prefix_len(path_a: &str, path_b: &str) -> usize {
    let parts_a: Vec<&str> = path_a.split('/').collect();
    let parts_b: Vec<&str> = path_b.split('/').collect();

    // Skip the basename of the current file if it's a file path
    let dirs_a = if parts_a.len() > 1 {
        &parts_a[..parts_a.len() - 1]
    } else {
        &parts_a[..]
    };

    let mut shared = 0;
    for (a, b) in dirs_a.iter().zip(parts_b.iter()) {
        if a.eq_ignore_ascii_case(b) {
            shared += 1;
        } else {
            break;
        }
    }
    shared
}

/// Owned entry used to render the repo outline after releasing the index lock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoOutlineFileView {
    pub relative_path: String,
    pub language: LanguageId,
    pub symbol_count: usize,
    /// Noise classification for suppressive filtering in explore/repo_map views.
    pub noise_class: crate::live_index::search::NoiseClass,
}

/// Owned compatibility/test view for file outline rendering.
///
/// Hot-path readers should prefer `capture_shared_file()` and format from `&IndexedFile`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileOutlineView {
    pub relative_path: String,
    pub symbols: Vec<SymbolRecord>,
}

/// Owned compatibility/test view for symbol detail rendering.
///
/// Hot-path readers should prefer `capture_shared_file()` and format from `&IndexedFile`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolDetailView {
    pub relative_path: String,
    pub content: Vec<u8>,
    pub symbols: Vec<SymbolRecord>,
}

/// Owned compatibility/test view for file content rendering.
///
/// Hot-path readers should prefer `capture_shared_file()` and format from `&IndexedFile`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileContentView {
    pub relative_path: String,
    pub content: Vec<u8>,
}

/// Owned timestamp/path view used by `what_changed` timestamp mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhatChangedTimestampView {
    pub loaded_secs: i64,
    pub paths: Vec<String>,
}

/// Owned path-resolution result for `search_files` resolve mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchFilesResolveView {
    EmptyHint,
    Resolved {
        path: String,
    },
    NotFound {
        hint: String,
    },
    Ambiguous {
        hint: String,
        matches: Vec<String>,
        overflow_count: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SearchFilesTier {
    CoChange,
    StrongPath,
    Basename,
    LoosePath,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SearchFilesCouplingEvidence {
    pub shared_commits: u32,
    pub weighted_score: f32,
}

pub type SearchFilesCouplingNeighbors = HashMap<String, SearchFilesCouplingEvidence>;

#[derive(Debug, Clone, PartialEq)]
pub struct SearchFilesHit {
    pub tier: SearchFilesTier,
    pub path: String,
    pub coupling_score: Option<f32>,
    pub shared_commits: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SearchFilesView {
    EmptyQuery,
    NotFound {
        query: String,
    },
    Found {
        query: String,
        total_matches: usize,
        overflow_count: usize,
        hits: Vec<SearchFilesHit>,
    },
}

fn usable_search_files_coupling_evidence(
    path: &str,
    query: &str,
    tokens: &[String],
    current_file: Option<&str>,
    coupling_context: Option<(&str, &SearchFilesCouplingNeighbors)>,
) -> Option<SearchFilesCouplingEvidence> {
    let (anchor_path, neighbors) = coupling_context?;
    let evidence = *neighbors.get(path)?;
    let ctx = super::rank_signals::RankCtx {
        query,
        tokens,
        current_file,
        target_path: Some(anchor_path),
        co_change_count: Some(evidence.shared_commits),
        co_change_weighted_score: Some(evidence.weighted_score),
    };
    let score = <super::rank_signals::CoChangeSignal as super::rank_signals::RankSignal>::score(
        &super::rank_signals::CoChangeSignal,
        std::path::Path::new(path),
        &ctx,
    );
    if score > 0.0 { Some(evidence) } else { None }
}

fn search_files_rank_score(
    path: &str,
    query: &str,
    tokens: &[String],
    current_file: Option<&str>,
    coupling_context: Option<(&str, &SearchFilesCouplingNeighbors)>,
) -> f32 {
    let evidence =
        usable_search_files_coupling_evidence(path, query, tokens, current_file, coupling_context);
    let ctx = super::rank_signals::RankCtx {
        query,
        tokens,
        current_file,
        target_path: coupling_context.map(|(anchor_path, _)| anchor_path),
        co_change_count: evidence.map(|evidence| evidence.shared_commits),
        co_change_weighted_score: evidence.map(|evidence| evidence.weighted_score),
    };
    super::rank_signals::combine(std::path::Path::new(path), &ctx)
}

/// One rendered dependent-reference line captured under the read lock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependentLineView {
    pub line_number: u32,
    pub line_content: String,
    pub kind: String,
    pub name: String,
}

/// One dependent file entry captured for `find_dependents`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependentFileView {
    pub file_path: String,
    pub lines: Vec<DependentLineView>,
}

/// Owned grouped view for `find_dependents`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindDependentsView {
    pub files: Vec<DependentFileView>,
}

/// One context line for a reference hit captured under the read lock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceContextLineView {
    pub line_number: u32,
    pub text: String,
    pub is_reference_line: bool,
    pub enclosing_annotation: Option<String>,
}

/// One reference hit with its surrounding context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceHitView {
    pub context_lines: Vec<ReferenceContextLineView>,
}

/// One file entry in a grouped references result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceFileView {
    pub file_path: String,
    pub hits: Vec<ReferenceHitView>,
}

/// Owned grouped view for `find_references`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindReferencesView {
    pub total_refs: usize,
    pub total_files: usize,
    pub files: Vec<ReferenceFileView>,
}

/// One entry in an implementations-mode `find_references` result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplementationEntryView {
    /// The trait/interface name.
    pub trait_name: String,
    /// The implementing type name.
    pub implementor: String,
    /// File where the implements reference was found.
    pub file_path: String,
    /// Line of the implements reference.
    pub line: u32,
}

/// Owned grouped view for implementations-mode `find_references`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplementationsView {
    pub entries: Vec<ImplementationEntryView>,
}

/// A sibling symbol at the same depth within the same file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiblingSymbolView {
    pub name: String,
    pub kind_label: String,
    pub line_range: (u32, u32),
}

/// Git activity snapshot for a single file (owned, display-ready).
#[derive(Debug, Clone, PartialEq)]
pub struct GitActivityView {
    pub churn_score: f32,
    pub churn_bar: String,
    pub churn_label: String,
    pub commit_count: u32,
    pub last_relative: String,
    pub last_hash: String,
    pub last_message: String,
    pub last_author: String,
    pub last_timestamp: String,
    pub owners: Vec<String>,
    pub co_changes: Vec<(String, f32, u32)>,
}

/// Full trace result for a single symbol.
#[derive(Debug, Clone, PartialEq)]
pub struct TraceSymbolFoundView {
    pub context_bundle: ContextBundleFoundView,
    pub dependents: FindDependentsView,
    pub siblings: Vec<SiblingSymbolView>,
    pub implementations: ImplementationsView,
    pub git_activity: Option<GitActivityView>,
}

/// Owned result view for `trace_symbol`.
#[derive(Debug, Clone, PartialEq)]
pub enum TraceSymbolView {
    FileNotFound {
        path: String,
    },
    AmbiguousSymbol {
        path: String,
        name: String,
        candidate_lines: Vec<u32>,
    },
    SymbolNotFound {
        relative_path: String,
        symbol_names: Vec<String>,
        name: String,
    },
    Found(Box<TraceSymbolFoundView>),
}

/// A focused symbol summary for `inspect_match`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnclosingSymbolView {
    pub name: String,
    pub kind_label: String,
    pub line_range: (u32, u32),
}

/// Found result for `inspect_match`.
#[derive(Debug, Clone, PartialEq)]
pub struct InspectMatchFoundView {
    pub path: String,
    pub line: u32,
    pub excerpt: String,
    pub enclosing: Option<EnclosingSymbolView>,
    /// Full parent chain from outermost (depth 0) to the enclosing symbol,
    /// e.g. [module, class, method] — gives full nesting context.
    pub parent_chain: Vec<EnclosingSymbolView>,
    pub siblings: Vec<SiblingSymbolView>,
    /// Number of siblings omitted due to the sibling_limit cap.
    pub siblings_overflow: usize,
}

/// Owned result view for `inspect_match`.
#[derive(Debug, Clone, PartialEq)]
pub enum InspectMatchView {
    FileNotFound {
        path: String,
    },
    LineOutOfBounds {
        path: String,
        line: u32,
        total_lines: usize,
    },
    Found(InspectMatchFoundView),
}

/// Owned repo outline view captured under a short read lock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoOutlineView {
    pub total_files: usize,
    pub total_symbols: usize,
    pub files: Vec<RepoOutlineFileView>,
}

impl LiveIndex {
    /// O(1) lookup of a file by its relative path.
    pub fn get_file(&self, relative_path: &str) -> Option<&IndexedFile> {
        self.files.get(relative_path).map(|file| file.as_ref())
    }

    /// Returns the symbol slice for a file, or an empty slice if not found.
    pub fn symbols_for_file(&self, relative_path: &str) -> &[SymbolRecord] {
        self.files
            .get(relative_path)
            .map(|file| file.symbols.as_slice())
            .unwrap_or(&[])
    }

    /// Iterate all (path, file) pairs in the index.
    pub fn all_files(&self) -> impl Iterator<Item = (&String, &IndexedFile)> {
        self.files.iter().map(|(path, file)| (path, file.as_ref()))
    }

    /// Capture a shared immutable file entry under the read lock.
    pub fn capture_shared_file(&self, relative_path: &str) -> Option<Arc<IndexedFile>> {
        self.files.get(relative_path).cloned()
    }

    /// Capture one shared immutable file entry selected by an internal path scope.
    pub fn capture_shared_file_for_scope(
        &self,
        path_scope: &PathScope,
    ) -> Option<Arc<IndexedFile>> {
        match path_scope {
            PathScope::Any => None,
            PathScope::Exact(path) => self.capture_shared_file(path),
            PathScope::Prefix(prefix) => {
                let mut matching_paths: Vec<&String> = self
                    .files
                    .keys()
                    .filter(|path| path.starts_with(prefix.as_str()))
                    .collect();
                matching_paths.sort_by_key(|p| p.len());
                matching_paths
                    .first()
                    .and_then(|p| self.capture_shared_file(p))
            }
        }
    }

    /// Capture an owned compatibility/test outline view.
    ///
    /// New hot-path readers should prefer `capture_shared_file()`.
    pub fn capture_file_outline_view(&self, relative_path: &str) -> Option<FileOutlineView> {
        let file = self.get_file(relative_path)?;
        Some(FileOutlineView {
            relative_path: file.relative_path.clone(),
            symbols: file.symbols.clone(),
        })
    }

    /// Capture an owned compatibility/test symbol-detail view.
    ///
    /// New hot-path readers should prefer `capture_shared_file()`.
    pub fn capture_symbol_detail_view(&self, relative_path: &str) -> Option<SymbolDetailView> {
        let file = self.get_file(relative_path)?;
        Some(SymbolDetailView {
            relative_path: file.relative_path.clone(),
            content: file.content.clone(),
            symbols: file.symbols.clone(),
        })
    }

    /// Capture an owned compatibility/test file-content view.
    ///
    /// New hot-path readers should prefer `capture_shared_file()`.
    pub fn capture_file_content_view(&self, relative_path: &str) -> Option<FileContentView> {
        let file = self.get_file(relative_path)?;
        Some(FileContentView {
            relative_path: file.relative_path.clone(),
            content: file.content.clone(),
        })
    }

    /// Capture the data needed for `what_changed` timestamp mode without holding the read lock.
    pub fn capture_what_changed_timestamp_view(&self) -> WhatChangedTimestampView {
        use std::time::UNIX_EPOCH;

        let loaded_secs = self
            .loaded_at_system()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let mut paths: Vec<String> = self.all_files().map(|(path, _)| path.clone()).collect();
        paths.sort();

        WhatChangedTimestampView { loaded_secs, paths }
    }

    /// Resolve a path hint to one exact indexed path, or a bounded ambiguous result.
    pub fn capture_search_files_resolve_view(&self, hint: &str) -> SearchFilesResolveView {
        self.capture_search_files_resolve_view_with_noise(hint, true, true)
    }

    /// Resolve a path hint while optionally suppressing high-noise path classes.
    pub fn capture_search_files_resolve_view_with_noise(
        &self,
        hint: &str,
        include_vendor: bool,
        include_personal_tooling: bool,
    ) -> SearchFilesResolveView {
        const RESOLVE_PATH_AMBIGUOUS_CAP: usize = 10;
        let normalized_hint = normalize_path_query(hint);
        if normalized_hint.is_empty() {
            return SearchFilesResolveView::EmptyHint;
        }
        let path_allowed = |path: &str| -> bool {
            (include_vendor || !is_vendor_path(path))
                && (include_personal_tooling || !is_personal_tooling_path(path))
        };

        if self.get_file(&normalized_hint).is_some() && path_allowed(&normalized_hint) {
            return SearchFilesResolveView::Resolved {
                path: normalized_hint,
            };
        }

        let normalized_hint_lower = normalized_hint.to_ascii_lowercase();
        let parts: Vec<&str> = normalized_hint
            .split('/')
            .filter(|part| !part.is_empty())
            .collect();
        let basename = parts.last().copied().unwrap_or("");
        let dir_components = if parts.len() > 1 {
            &parts[..parts.len() - 1]
        } else {
            &[][..]
        };

        let mut candidates: Vec<String> = self
            .find_files_by_basename(basename)
            .into_iter()
            .filter(|path| path_allowed(path))
            .map(|path| path.to_string())
            .collect();

        if candidates.is_empty() {
            candidates = self
                .all_files()
                .map(|(path, _)| path.as_str())
                .filter(|path| path_allowed(path))
                .filter(|path| {
                    let path_lower = path.to_ascii_lowercase();
                    path_lower.ends_with(&normalized_hint_lower)
                        || path_lower.contains(&normalized_hint_lower)
                })
                .map(|path| path.to_string())
                .collect();
        }

        for component in dir_components {
            let component_matches: HashSet<&str> = self
                .find_files_by_dir_component(component)
                .into_iter()
                .collect();
            candidates.retain(|path| component_matches.contains(path.as_str()));
        }

        candidates.sort_by(|left, right| {
            let left_lower = left.to_ascii_lowercase();
            let right_lower = right.to_ascii_lowercase();
            let left_suffix = left_lower.ends_with(&normalized_hint_lower);
            let right_suffix = right_lower.ends_with(&normalized_hint_lower);
            right_suffix
                .cmp(&left_suffix)
                .then(left.len().cmp(&right.len()))
                .then(left.cmp(right))
        });
        candidates.dedup();

        match candidates.len() {
            0 => SearchFilesResolveView::NotFound {
                hint: normalized_hint,
            },
            1 => SearchFilesResolveView::Resolved {
                path: candidates.pop().expect("single candidate"),
            },
            len => {
                let overflow_count = len.saturating_sub(RESOLVE_PATH_AMBIGUOUS_CAP);
                SearchFilesResolveView::Ambiguous {
                    hint: normalized_hint,
                    matches: candidates
                        .into_iter()
                        .take(RESOLVE_PATH_AMBIGUOUS_CAP)
                        .collect(),
                    overflow_count,
                }
            }
        }
    }

    /// Search for indexed files matching a path query with bounded tiered output.
    pub fn capture_search_files_view(
        &self,
        query: &str,
        limit: usize,
        current_file: Option<&str>,
        coupling_context: Option<(&str, &SearchFilesCouplingNeighbors)>,
    ) -> SearchFilesView {
        self.capture_search_files_view_with_noise(
            query,
            limit,
            current_file,
            coupling_context,
            true,
            true,
        )
    }

    /// Search for indexed files, optionally suppressing high-noise path classes.
    ///
    /// `capture_search_files_view` preserves the historical in-process behavior
    /// for tests and internal callers. Public tool entry points should call this
    /// variant so advertised vendor/personal-tooling defaults apply before
    /// ranking and total-match counts are computed.
    pub fn capture_search_files_view_with_noise(
        &self,
        query: &str,
        limit: usize,
        current_file: Option<&str>,
        coupling_context: Option<(&str, &SearchFilesCouplingNeighbors)>,
        include_vendor: bool,
        include_personal_tooling: bool,
    ) -> SearchFilesView {
        let limit = limit.clamp(1, 50);
        let normalized_query = normalize_path_query(query);
        if normalized_query.is_empty() {
            return SearchFilesView::EmptyQuery;
        }
        let path_allowed = |path: &str| -> bool {
            (include_vendor || !is_vendor_path(path))
                && (include_personal_tooling || !is_personal_tooling_path(path))
        };

        // Detect glob patterns and handle them with globset.
        let is_glob = normalized_query.contains('*')
            || normalized_query.contains('?')
            || normalized_query.contains('[');
        if is_glob
            && let Ok(glob) = globset::GlobBuilder::new(&normalized_query)
                .literal_separator(false)
                .build()
        {
            let matcher = glob.compile_matcher();
            let mut glob_hits: Vec<String> = self
                .all_files()
                .map(|(path, _)| path.as_str())
                .filter(|path| path_allowed(path))
                .filter(|path| matcher.is_match(path))
                .map(|path| path.to_string())
                .collect();
            glob_hits.sort();
            let total_matches = glob_hits.len();
            if total_matches == 0 {
                return SearchFilesView::NotFound {
                    query: normalized_query,
                };
            }
            let overflow_count = total_matches.saturating_sub(limit);
            glob_hits.truncate(limit);
            let hits: Vec<SearchFilesHit> = glob_hits
                .into_iter()
                .map(|path| SearchFilesHit {
                    tier: SearchFilesTier::StrongPath,
                    path,
                    coupling_score: None,
                    shared_commits: None,
                })
                .collect();
            return SearchFilesView::Found {
                query: normalized_query,
                total_matches,
                overflow_count,
                hits,
            };
            // If glob parsing fails, fall through to normal search.
        }

        let normalized_query_lower = normalized_query.to_ascii_lowercase();
        let tokens = tokenize_path_query(&normalized_query);
        let basename_token = tokens.last().map(String::as_str).unwrap_or("");
        let component_tokens = if tokens.len() > 1 {
            &tokens[..tokens.len() - 1]
        } else {
            &[][..]
        };
        let has_path_context = normalized_query.contains('/');

        // Classify each indexed path into its tier. Order matters: a path that
        // qualifies for a higher tier MUST NOT be double-counted in a lower one.
        // The tier labels are consumed by the formatter; the ordering across
        // tiers is driven by `super::rank_signals::combine` below.
        let mut strong_hits: Vec<String> = self
            .all_files()
            .map(|(path, _)| path.as_str())
            .filter(|path| path_allowed(path))
            .filter(|path| {
                let path_lower = path.to_ascii_lowercase();
                path_lower == normalized_query_lower
                    || (has_path_context && path_lower.ends_with(&normalized_query_lower))
            })
            .map(|path| path.to_string())
            .collect();

        let basename_hits: Vec<String> = if basename_token.is_empty() {
            Vec::new()
        } else {
            self.find_files_by_basename(basename_token)
                .into_iter()
                .filter(|path| path_allowed(path))
                .map(|path| path.to_string())
                .collect()
        };

        if !component_tokens.is_empty() {
            strong_hits.extend(
                basename_hits
                    .iter()
                    .filter(|path| {
                        component_tokens
                            .iter()
                            .all(|component| path_has_component(path, component))
                    })
                    .cloned(),
            );
        }

        // Dedup strong_hits (the basename+components extension above can
        // re-introduce paths already present via the exact/suffix filter).
        {
            let mut seen: HashSet<String> = HashSet::new();
            strong_hits.retain(|p| seen.insert(p.clone()));
        }

        let strong_set: HashSet<&str> = strong_hits.iter().map(String::as_str).collect();
        let basename_only_hits: Vec<String> = basename_hits
            .into_iter()
            .filter(|path| !strong_set.contains(path.as_str()))
            .collect();

        // Prefix matches on basenames (e.g., "orchestrat" matches "orchestrator.rs" and "orchestration.rs")
        let basename_set: HashSet<&str> = strong_hits
            .iter()
            .chain(basename_only_hits.iter())
            .map(String::as_str)
            .collect();
        let prefix_hits: Vec<String> = if basename_token.len() >= 3 {
            let basename_token_lower = basename_token.to_ascii_lowercase();
            self.all_files()
                .map(|(path, _)| path.as_str())
                .filter(|path| path_allowed(path))
                .filter(|path| {
                    let file_basename = std::path::Path::new(path)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("");
                    file_basename
                        .to_ascii_lowercase()
                        .starts_with(&basename_token_lower)
                })
                .filter(|path| !basename_set.contains(path))
                .map(|path| path.to_string())
                .collect()
        } else {
            Vec::new()
        };

        let strong_or_basename_or_prefix_set: HashSet<&str> = strong_hits
            .iter()
            .chain(basename_only_hits.iter())
            .chain(prefix_hits.iter())
            .map(String::as_str)
            .collect();
        let loose_hits: Vec<String> = self
            .all_files()
            .map(|(path, _)| path.as_str())
            .filter(|path| path_allowed(path))
            .filter(|path| !strong_or_basename_or_prefix_set.contains(*path))
            .filter(|path| {
                let path_lower = path.to_ascii_lowercase();
                tokens.iter().all(|token| path_lower.contains(token))
            })
            .map(|path| path.to_string())
            .collect();

        let total_matches =
            strong_hits.len() + basename_only_hits.len() + prefix_hits.len() + loose_hits.len();
        if total_matches == 0 {
            return SearchFilesView::NotFound {
                query: normalized_query,
            };
        }

        // Collect all classified candidates into a single list, preserving
        // their tier label for the formatter. Ordering across the list is
        // then driven by `super::rank_signals::combine` as the primary sort key,
        // with within-tier tiebreakers (exact/suffix, shared dir prefix,
        // length, lex) matching the previous per-bucket behavior.
        let mut candidates: Vec<(String, SearchFilesTier)> = Vec::with_capacity(total_matches);
        candidates.extend(
            strong_hits
                .into_iter()
                .map(|path| (path, SearchFilesTier::StrongPath)),
        );
        candidates.extend(
            basename_only_hits
                .into_iter()
                .map(|path| (path, SearchFilesTier::Basename)),
        );
        candidates.extend(
            prefix_hits
                .into_iter()
                .map(|path| (path, SearchFilesTier::LoosePath)),
        );
        candidates.extend(
            loose_hits
                .into_iter()
                .map(|path| (path, SearchFilesTier::LoosePath)),
        );

        let shared_len = |path: &str| -> usize {
            current_file
                .map(|cur| shared_directory_prefix_len(cur, path))
                .unwrap_or(0)
        };

        let coupling_context = coupling_context
            .filter(|(anchor_path, neighbors)| !anchor_path.is_empty() && !neighbors.is_empty());
        let max_coupling_weight = coupling_context
            .map(|(_, neighbors)| {
                neighbors
                    .iter()
                    .filter(|(path, _)| path_allowed(path.as_str()))
                    .map(|(_, evidence)| evidence.weighted_score)
                    .filter(|score| score.is_finite() && *score > 0.0)
                    .fold(0.0_f32, f32::max)
            })
            .unwrap_or(0.0);
        candidates.sort_by(|(lp, _), (rp, _)| {
            let l_score = search_files_rank_score(
                lp,
                &normalized_query,
                &tokens,
                current_file,
                coupling_context,
            );
            let r_score = search_files_rank_score(
                rp,
                &normalized_query,
                &tokens,
                current_file,
                coupling_context,
            );
            let l_lower = lp.to_ascii_lowercase();
            let r_lower = rp.to_ascii_lowercase();
            let l_exact = l_lower == normalized_query_lower;
            let r_exact = r_lower == normalized_query_lower;
            let l_suffix = has_path_context && l_lower.ends_with(&normalized_query_lower);
            let r_suffix = has_path_context && r_lower.ends_with(&normalized_query_lower);
            r_score
                .partial_cmp(&l_score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(r_exact.cmp(&l_exact))
                .then(r_suffix.cmp(&l_suffix))
                .then(shared_len(rp).cmp(&shared_len(lp)))
                .then(lp.len().cmp(&rp.len()))
                .then(lp.cmp(rp))
        });

        let overflow_count = total_matches.saturating_sub(limit);
        candidates.truncate(limit);

        let hits: Vec<SearchFilesHit> = candidates
            .into_iter()
            .map(|(path, tier)| {
                let coupling_evidence = usable_search_files_coupling_evidence(
                    &path,
                    &normalized_query,
                    &tokens,
                    current_file,
                    coupling_context,
                );
                let coupling_score = coupling_evidence.and_then(|evidence| {
                    if max_coupling_weight > 0.0 {
                        Some((evidence.weighted_score / max_coupling_weight).clamp(0.0, 1.0))
                    } else {
                        None
                    }
                });
                SearchFilesHit {
                    tier: if coupling_evidence.is_some() {
                        SearchFilesTier::CoChange
                    } else {
                        tier
                    },
                    path,
                    coupling_score,
                    shared_commits: coupling_evidence.map(|evidence| evidence.shared_commits),
                }
            })
            .collect();

        SearchFilesView::Found {
            query: normalized_query,
            total_matches,
            overflow_count,
            hits,
        }
    }

    /// Capture the grouped data needed for `find_dependents` without holding the read lock.
    pub fn capture_find_dependents_view(&self, target_path: &str) -> FindDependentsView {
        let deps = self.find_dependents_for_file(target_path);
        let mut by_file: std::collections::BTreeMap<String, Vec<DependentLineView>> =
            std::collections::BTreeMap::new();

        for (file_path, reference) in deps {
            let line_number = reference.line_range.0 + 1;
            let line_content = self
                .get_file(file_path)
                .map(|file| {
                    String::from_utf8_lossy(&file.content)
                        .lines()
                        .nth(reference.line_range.0 as usize)
                        .unwrap_or("")
                        .to_string()
                })
                .unwrap_or_default();

            by_file
                .entry(file_path.to_string())
                .or_default()
                .push(DependentLineView {
                    line_number,
                    line_content,
                    kind: reference.kind.to_string(),
                    name: reference.name.clone(),
                });
        }

        FindDependentsView {
            files: by_file
                .into_iter()
                .map(|(file_path, lines)| DependentFileView { file_path, lines })
                .collect(),
        }
    }

    /// Capture the grouped data needed for `find_references` without holding the read lock.
    pub fn capture_find_references_view(
        &self,
        name: &str,
        kind_filter: Option<&str>,
        total_limit: usize,
    ) -> FindReferencesView {
        let kind_enum = parse_reference_kind_filter(kind_filter);
        let refs = self.find_references_for_name(name, kind_enum, false);
        self.build_find_references_view(&refs, total_limit)
    }

    /// Find all implementations of a trait/interface, or all traits a type implements.
    ///
    /// `name` is the trait or type to search for.
    /// `direction`: `None` or `Some("auto")` searches both directions.
    ///   `Some("trait")` treats `name` as a trait and returns implementors.
    ///   `Some("type")` treats `name` as a type and returns its traits.
    pub fn capture_implementations_view(
        &self,
        name: &str,
        direction: Option<&str>,
    ) -> ImplementationsView {
        let mut entries: Vec<ImplementationEntryView> = Vec::new();

        for (file_path, file) in &self.files {
            for reference in &file.references {
                if reference.kind != ReferenceKind::Implements {
                    continue;
                }
                let trait_name = &reference.name;
                let implementor = match &reference.qualified_name {
                    Some(qn) => qn,
                    None => continue,
                };

                let matches = match direction {
                    Some("trait") => trait_name == name,
                    Some("type") => implementor == name,
                    _ => trait_name == name || implementor == name,
                };

                if matches {
                    entries.push(ImplementationEntryView {
                        trait_name: trait_name.clone(),
                        implementor: implementor.clone(),
                        file_path: file_path.clone(),
                        line: reference.line_range.0 + 1,
                    });
                }
            }
        }

        // Sort: group by trait name, then by implementor
        entries.sort_by(|a, b| {
            a.trait_name
                .cmp(&b.trait_name)
                .then(a.implementor.cmp(&b.implementor))
        });

        ImplementationsView { entries }
    }

    pub fn capture_find_references_view_for_symbol(
        &self,
        path: &str,
        name: &str,
        symbol_kind: Option<&str>,
        symbol_line: Option<u32>,
        kind_filter: Option<&str>,
        total_limit: usize,
    ) -> Result<FindReferencesView, String> {
        let kind_enum = parse_reference_kind_filter(kind_filter);
        let refs =
            self.find_exact_references_for_symbol(path, name, symbol_kind, symbol_line, kind_enum)?;
        Ok(self.build_find_references_view(&refs, total_limit))
    }

    pub fn find_exact_references_for_symbol<'a>(
        &'a self,
        path: &'a str,
        name: &str,
        symbol_kind: Option<&str>,
        symbol_line: Option<u32>,
        kind_filter: Option<ReferenceKind>,
    ) -> Result<Vec<(&'a str, &'a ReferenceRecord)>, String> {
        let Some(file) = self.get_file(path) else {
            return Err(format!("File not found: {path}"));
        };

        match resolve_symbol_selector(file, name, symbol_kind, symbol_line) {
            SymbolSelectorMatch::NotFound => {
                let selector = render_symbol_selector(name, symbol_kind, symbol_line);
                Err(format!("Symbol not found in {path}: {selector}"))
            }
            SymbolSelectorMatch::Ambiguous(candidate_lines) => {
                let candidate_lines = candidate_lines
                    .iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                Err(format!(
                    "Ambiguous symbol selector for {name} in {path}; pass `symbol_line` to disambiguate. Candidates: {candidate_lines}"
                ))
            }
            SymbolSelectorMatch::Selected(_, symbol) => {
                Ok(self.collect_exact_symbol_references(path, file, symbol, kind_filter))
            }
        }
    }

    pub(super) fn collect_exact_symbol_references<'a>(
        &'a self,
        path: &'a str,
        file: &'a IndexedFile,
        target_symbol: &'a SymbolRecord,
        kind_filter: Option<ReferenceKind>,
    ) -> Vec<(&'a str, &'a ReferenceRecord)> {
        let target_name = target_symbol.name.as_str();
        let module_path = resolve_module_path(path, &file.language);

        // Index of the target symbol within its own file's symbol list. Used to
        // suppress the SF-002 self-member-call false positive: a method whose
        // body calls a same-named member on another object (e.g. TS
        // `this.service.foo()` from `Controller.foo`) must not be reported as
        // its own caller. We reject a same-file `Call` ref only when it is
        // BOTH enclosed by the target itself AND a receiver method call. Bare
        // intra-body recursion (`foo()`) and same-named `this.foo()` calls from
        // OTHER methods are deliberately preserved as legitimate callers.
        let target_symbol_index = file
            .symbols
            .iter()
            .position(|symbol| symbol.byte_range == target_symbol.byte_range)
            .map(|idx| idx as u32);

        let mut refs: Vec<(&str, &ReferenceRecord)> = file
            .references
            .iter()
            .filter(|reference| {
                if reference.kind == ReferenceKind::Call
                    && target_symbol_index.is_some()
                    && reference.enclosing_symbol_index == target_symbol_index
                    && is_receiver_method_call(file, reference)
                {
                    return false;
                }
                matches_exact_symbol_reference(
                    reference,
                    target_name,
                    &file.language,
                    target_symbol.kind,
                    module_path.as_deref(),
                    Some(file),
                    kind_filter,
                )
            })
            .map(|reference| (path, reference))
            .collect();

        let dependent_refs = self.find_dependents_for_file(path);
        let dependent_paths: HashSet<&str> = dependent_refs.iter().map(|(fp, _)| *fp).collect();

        refs.extend(dependent_refs.into_iter().filter(|(file_path, reference)| {
            let reference_file = self.get_file(file_path);
            matches_exact_symbol_reference(
                reference,
                target_name,
                &file.language,
                target_symbol.kind,
                module_path.as_deref(),
                reference_file,
                kind_filter,
            )
        }));

        // Also check the reverse index for the target name.
        for (ref_path, ref_record) in self.find_references_for_name(target_name, kind_filter, false)
        {
            if ref_path == path {
                continue;
            }
            // Already collected from the dependent scan?
            if refs
                .iter()
                .any(|(p, r)| *p == ref_path && r.byte_range == ref_record.byte_range)
            {
                continue;
            }

            let reference_matches = matches_exact_symbol_reference(
                ref_record,
                target_name,
                &file.language,
                target_symbol.kind,
                module_path.as_deref(),
                self.get_file(ref_path),
                kind_filter,
            );

            if !reference_matches {
                continue;
            }

            if dependent_paths.contains(ref_path) {
                // Within known dependents: accept any matching reference (original behavior)
                refs.push((ref_path, ref_record));
            } else if let Some(qn) = ref_record.qualified_name.as_deref() {
                // Outside dependents: only accept if the reference has a qualified name
                // that matches the target's module path.  This catches fully-qualified
                // calls (e.g. `engine::optimize_deterministic()`) where the caller has
                // no separate `use` import and therefore never enters dependent_paths.
                // Simple name-only matches are excluded to avoid false positives from
                // unrelated files that happen to use the same function name.
                if matches_exact_symbol_qualified_name(
                    &file.language,
                    qn,
                    target_name,
                    module_path.as_deref(),
                ) {
                    refs.push((ref_path, ref_record));
                }
            }
        }

        refs.sort_by(|a, b| {
            a.0.cmp(b.0)
                .then(a.1.line_range.0.cmp(&b.1.line_range.0))
                .then(a.1.byte_range.0.cmp(&b.1.byte_range.0))
        });

        refs
    }

    fn build_find_references_view(
        &self,
        refs: &[(&str, &ReferenceRecord)],
        total_limit: usize,
    ) -> FindReferencesView {
        let mut by_file: std::collections::BTreeMap<String, Vec<ReferenceHitView>> =
            std::collections::BTreeMap::new();

        let mut built = 0usize;
        for (file_path, reference) in refs {
            if built >= total_limit {
                break;
            }
            let Some(file) = self.get_file(file_path) else {
                continue;
            };
            let content = String::from_utf8_lossy(&file.content);
            let content_lines: Vec<&str> = content.lines().collect();
            let ref_line_0 = reference.line_range.0 as usize;
            let ctx_start = ref_line_0.saturating_sub(1);
            let ctx_end = if content_lines.is_empty() {
                0
            } else {
                (ref_line_0 + 1).min(content_lines.len() - 1)
            };
            let enclosing_annotation = reference
                .enclosing_symbol_index
                .and_then(|idx| file.symbols.get(idx as usize))
                .map(|sym| format!("  [in {} {}]", sym.kind, sym.name));

            let context_lines = if content_lines.is_empty() {
                Vec::new()
            } else {
                content_lines[ctx_start..=ctx_end]
                    .iter()
                    .enumerate()
                    .map(|(i, line)| {
                        let zero_based_line = ctx_start + i;
                        ReferenceContextLineView {
                            line_number: (zero_based_line + 1) as u32,
                            text: (*line).to_string(),
                            is_reference_line: zero_based_line == ref_line_0,
                            enclosing_annotation: if zero_based_line == ref_line_0 {
                                enclosing_annotation.clone()
                            } else {
                                None
                            },
                        }
                    })
                    .collect()
            };

            by_file
                .entry((*file_path).to_string())
                .or_default()
                .push(ReferenceHitView { context_lines });
            built += 1;
        }

        let total_files = refs
            .iter()
            .map(|(f, _)| *f)
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        FindReferencesView {
            total_refs: refs.len(),
            total_files,
            files: by_file
                .into_iter()
                .map(|(file_path, hits)| ReferenceFileView { file_path, hits })
                .collect(),
        }
    }

    /// Capture a full trace view for a symbol, composing existing captures.
    ///
    /// This is the one-call semantic investigation that replaces the common
    /// search_symbols → get_symbol_context(bundle) → find_dependents → get_file_context pattern.
    pub fn capture_trace_symbol_view(
        &self,
        path: &str,
        name: &str,
        kind_filter: Option<&str>,
        symbol_line: Option<u32>,
        sections: Option<&[String]>,
        _include_tests: bool,
    ) -> TraceSymbolView {
        // Reuse context_bundle for the core symbol + callers + callees + type_usages + deps.
        let bundle = self.capture_context_bundle_view(path, name, kind_filter, symbol_line);

        let found = match bundle {
            ContextBundleView::FileNotFound { path } => {
                return TraceSymbolView::FileNotFound { path };
            }
            ContextBundleView::AmbiguousSymbol {
                path,
                name,
                candidate_lines,
            } => {
                return TraceSymbolView::AmbiguousSymbol {
                    path,
                    name,
                    candidate_lines,
                };
            }
            ContextBundleView::SymbolNotFound {
                relative_path,
                symbol_names,
                name,
            } => {
                return TraceSymbolView::SymbolNotFound {
                    relative_path,
                    symbol_names,
                    name,
                };
            }
            ContextBundleView::Found(view) => *view,
        };

        let wants = |section: &str| -> bool {
            sections
                .map(|s| s.iter().any(|v| v.eq_ignore_ascii_case(section)))
                .unwrap_or(true)
        };

        // Dependents: files that import the target file.
        let dependents = if wants("dependents") {
            self.capture_find_dependents_view(path)
        } else {
            FindDependentsView { files: vec![] }
        };

        // Siblings: symbols at the same depth in the same file.
        let siblings = if wants("siblings") {
            self.get_file(path)
                .map(|file| {
                    // Find the target symbol to get its depth.
                    let target_depth = file
                        .symbols
                        .iter()
                        .find(|s| {
                            s.name == name
                                && kind_filter
                                    .map(|k| s.kind.to_string().eq_ignore_ascii_case(k))
                                    .unwrap_or(true)
                                && symbol_line.map(|l| s.line_range.0 + 1 == l).unwrap_or(true)
                        })
                        .map(|s| s.depth)
                        .unwrap_or(0);

                    file.symbols
                        .iter()
                        .filter(|s| s.depth == target_depth && s.name != name)
                        .map(|s| SiblingSymbolView {
                            name: s.name.clone(),
                            kind_label: s.kind.to_string(),
                            line_range: (s.line_range.0 + 1, s.line_range.1 + 1),
                        })
                        .collect()
                })
                .unwrap_or_default()
        } else {
            vec![]
        };

        // Trait implementations.
        let implementations = if wants("implementations") {
            self.capture_implementations_view(name, None)
        } else {
            ImplementationsView { entries: vec![] }
        };

        TraceSymbolView::Found(Box::new(TraceSymbolFoundView {
            context_bundle: found,
            dependents,
            siblings,
            implementations,
            git_activity: None, // Filled in by the tool method which has access to git_temporal.
        }))
    }

    /// Capture a focused inspection view around a specific line.
    pub fn capture_inspect_match_view(
        &self,
        path: &str,
        line: u32,
        context_lines: Option<u32>,
        sibling_limit: Option<u32>,
    ) -> InspectMatchView {
        let Some(file) = self.get_file(path) else {
            return InspectMatchView::FileNotFound {
                path: path.to_string(),
            };
        };

        // 1. Render excerpt (simple around-line logic).
        let content = String::from_utf8_lossy(&file.content);
        let lines: Vec<&str> = content.lines().collect();

        if line as usize > lines.len() || line == 0 {
            return InspectMatchView::LineOutOfBounds {
                path: file.relative_path.clone(),
                line,
                total_lines: lines.len(),
            };
        }

        let anchor = line as usize;
        let context = context_lines.unwrap_or(3) as usize;
        let start = anchor.saturating_sub(context).max(1);
        let end = anchor.saturating_add(context).min(lines.len());

        let excerpt = if start > end || start > lines.len() {
            String::new()
        } else {
            (start..=end)
                .map(|ln| format!("{ln}: {}", lines[ln - 1]))
                .collect::<Vec<_>>()
                .join("\n")
        };

        // 2. Find enclosing symbol (deepest symbol containing the line).
        // `line` input is 1-based, symbol ranges are 0-based inclusive.
        let target_line_0 = line.saturating_sub(1);
        let enclosing_symbol = file
            .symbols
            .iter()
            .filter(|s| s.line_range.0 <= target_line_0 && s.line_range.1 >= target_line_0)
            .max_by_key(|s| s.depth);

        let enclosing = enclosing_symbol.map(|s| EnclosingSymbolView {
            name: s.name.clone(),
            kind_label: s.kind.to_string(),
            line_range: (s.line_range.0 + 1, s.line_range.1 + 1),
        });

        // 2b. Build parent chain: all enclosing symbols sorted by depth
        // (outermost first), e.g. [module, class, method].
        let mut parent_chain: Vec<EnclosingSymbolView> = {
            let mut by_depth: std::collections::BTreeMap<u32, &SymbolRecord> =
                std::collections::BTreeMap::new();
            for s in file
                .symbols
                .iter()
                .filter(|s| s.line_range.0 <= target_line_0 && s.line_range.1 >= target_line_0)
            {
                by_depth
                    .entry(s.depth)
                    .and_modify(|existing| {
                        // Keep the tightest (smallest) range at each depth.
                        let existing_span = existing.line_range.1 - existing.line_range.0;
                        let new_span = s.line_range.1 - s.line_range.0;
                        if new_span < existing_span {
                            *existing = s;
                        }
                    })
                    .or_insert(s);
            }
            by_depth
                .into_values()
                .map(|s| EnclosingSymbolView {
                    name: s.name.clone(),
                    kind_label: s.kind.to_string(),
                    line_range: (s.line_range.0 + 1, s.line_range.1 + 1),
                })
                .collect()
        };
        parent_chain.sort_by_key(|v| v.line_range.0);

        // 3. Find siblings (same depth as enclosing, or depth 0).
        let target_depth = enclosing_symbol.map(|s| s.depth).unwrap_or(0);
        let limit = sibling_limit.unwrap_or(10) as usize;
        let mut siblings: Vec<SiblingSymbolView> = file
            .symbols
            .iter()
            .filter(|s| s.depth == target_depth)
            .map(|s| SiblingSymbolView {
                name: s.name.clone(),
                kind_label: s.kind.to_string(),
                line_range: (s.line_range.0 + 1, s.line_range.1 + 1),
            })
            .collect();
        let siblings_overflow = if limit == 0 {
            // sibling_limit=0 means suppress siblings entirely — no overflow hint either.
            siblings.clear();
            0
        } else if siblings.len() > limit {
            let overflow = siblings.len() - limit;
            siblings.truncate(limit);
            overflow
        } else {
            0
        };

        InspectMatchView::Found(InspectMatchFoundView {
            path: file.relative_path.clone(),
            line,
            excerpt,
            enclosing,
            parent_chain,
            siblings,
            siblings_overflow,
        })
    }

    /// Capture the data needed to render `repo_outline` without holding the read lock.
    pub fn capture_repo_outline_view(&self) -> RepoOutlineView {
        use crate::live_index::search::NoisePolicy;
        let gi_ref = self.gitignore.as_ref();
        let mut files: Vec<RepoOutlineFileView> = self
            .all_files()
            .map(|(relative_path, file)| RepoOutlineFileView {
                noise_class: NoisePolicy::classify_path(relative_path, gi_ref),
                relative_path: relative_path.clone(),
                language: file.language.clone(),
                symbol_count: file.symbols.len(),
            })
            .collect();
        files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));

        let total_symbols = files.iter().map(|file| file.symbol_count).sum();

        RepoOutlineView {
            total_files: files.len(),
            total_symbols,
            files,
        }
    }

    /// Return sorted relative paths matching a basename, case-insensitively.
    pub fn find_files_by_basename(&self, basename: &str) -> Vec<&str> {
        self.files_by_basename
            .get(&basename.to_ascii_lowercase())
            .map(|paths| paths.iter().map(|path| path.as_str()).collect())
            .unwrap_or_default()
    }

    /// Return sorted relative paths containing the given directory component, case-insensitively.
    pub fn find_files_by_dir_component(&self, component: &str) -> Vec<&str> {
        self.files_by_dir_component
            .get(&component.to_ascii_lowercase())
            .map(|paths| paths.iter().map(|path| path.as_str()).collect())
            .unwrap_or_default()
    }

    // -----------------------------------------------------------------------
    // Cross-reference query methods (Phase 4, Plan 02)
    // -----------------------------------------------------------------------

    /// Find all `ReferenceRecord`s across the repo that match `name`.
    ///
    /// # Arguments
    /// * `name` — the reference name to look up. If it contains `::` or `.`,
    ///   it is treated as a qualified name and matched against `qualified_name`.
    ///   Otherwise matched against `name`.
    /// * `kind_filter` — when `Some(k)`, only references of kind `k` are returned.
    /// * `include_filtered` — when `false` (default), built-in type names and
    ///   single-letter generic parameters are silently filtered out (returns empty).
    ///   Set to `true` to bypass that filter.
    ///
    /// # Alias resolution (XREF-05)
    /// In addition to the direct reverse-index lookup, the method also checks
    /// every file's `alias_map`. If a file declares `alias_map["Map"] = "HashMap"`,
    /// then searching for `"HashMap"` will also yield references stored under `"Map"`.
    ///
    /// Returns a `Vec` of `(file_path, &ReferenceRecord)` tuples.
    pub fn find_references_for_name(
        &self,
        name: &str,
        kind_filter: Option<ReferenceKind>,
        include_filtered: bool,
    ) -> Vec<(&str, &ReferenceRecord)> {
        let is_qualified = name.contains("::") || name.contains('.');

        let mut results: Vec<(&str, &ReferenceRecord)> = Vec::new();

        if is_qualified {
            // Qualified lookup: the reverse index is keyed by simple name, not qualified name.
            // We must scan all files and match against the qualified_name field.
            for (file_path, file) in &self.files {
                for reference in &file.references {
                    if let Some(qn) = reference.qualified_name.as_deref() {
                        if qn != name {
                            continue;
                        }
                    } else {
                        continue;
                    }
                    if let Some(kf) = kind_filter
                        && reference.kind != kf
                    {
                        continue;
                    }
                    if !include_filtered && is_filtered_name(&reference.name, &file.language) {
                        continue;
                    }
                    results.push((file_path.as_str(), reference));
                }
            }
        } else {
            // Simple lookup: use the reverse index for O(1) name lookup.
            self.collect_refs_for_key(name, kind_filter, include_filtered, &mut results);

            // Alias resolution: find any alias that resolves to `name`.
            // e.g. alias_map["Map"] = "HashMap" means we also look up "Map".
            // Collect aliases first to avoid re-borrowing self during mutation of results.
            let aliases: Vec<String> = self
                .files
                .values()
                .flat_map(|file| {
                    file.alias_map
                        .iter()
                        .filter(|(_alias, original)| original.as_str() == name)
                        .map(|(alias, _)| alias.clone())
                })
                .collect();

            for alias in &aliases {
                self.collect_refs_for_key(alias, kind_filter, include_filtered, &mut results);
            }
        }

        results
    }

    /// Internal helper: look up `lookup_key` in `reverse_index`, resolve each location,
    /// apply kind filter (no qualified-name check), and append matching results.
    ///
    /// Only used for simple (non-qualified) name lookups.
    fn collect_refs_for_key<'a>(
        &'a self,
        lookup_key: &str,
        kind_filter: Option<ReferenceKind>,
        include_filtered: bool,
        results: &mut Vec<(&'a str, &'a ReferenceRecord)>,
    ) {
        if let Some(locations) = self.reverse_index.get(lookup_key) {
            for loc in locations {
                let file = match self.files.get(&loc.file_path) {
                    Some(f) => f,
                    None => continue,
                };
                let reference = match file.references.get(loc.reference_idx as usize) {
                    Some(r) => r,
                    None => continue,
                };
                if let Some(kf) = kind_filter
                    && reference.kind != kf
                {
                    continue;
                }
                if !include_filtered && is_filtered_name(&reference.name, &file.language) {
                    continue;
                }
                results.push((loc.file_path.as_str(), reference));
            }
        }
    }

    /// Check whether a file exports a public symbol with the given name.
    /// Uses a text scan of file content since SymbolRecord has no visibility field.
    pub(crate) fn has_pub_symbol(file: &IndexedFile, name: &str) -> bool {
        let is_word_match = |content: &str, pattern: &str| -> bool {
            let mut start = 0;
            while let Some(pos) = content[start..].find(pattern) {
                let abs_pos = start + pos;
                let after = abs_pos + pattern.len();
                if after >= content.len() {
                    return true;
                }
                let ch = content.as_bytes()[after];
                if !(ch.is_ascii_alphanumeric() || ch == b'_') {
                    return true;
                }
                start = abs_pos + 1;
            }
            false
        };

        match file.language {
            LanguageId::Rust => {
                let content = String::from_utf8_lossy(&file.content);
                for keyword in &[
                    "fn", "struct", "enum", "trait", "type", "const", "static", "mod",
                ] {
                    for vis in &["pub ", "pub(crate) ", "pub(super) ", "pub(self) "] {
                        if is_word_match(&content, &format!("{vis}{keyword} {name}")) {
                            return true;
                        }
                    }
                }
                false
            }
            LanguageId::JavaScript | LanguageId::TypeScript => {
                let content = String::from_utf8_lossy(&file.content);
                is_word_match(&content, &format!("export {{ {name}"))
                    || is_word_match(&content, &format!("export {name}"))
                    || is_word_match(&content, &format!("export default {name}"))
                    || is_word_match(&content, &format!("export function {name}"))
                    || is_word_match(&content, &format!("export class {name}"))
                    || is_word_match(&content, &format!("export const {name}"))
                    || is_word_match(&content, &format!("export interface {name}"))
                    || is_word_match(&content, &format!("export type {name}"))
            }
            // Python: all module-level symbols are importable, skip filter
            // Other languages: skip filter to avoid false negatives
            _ => true,
        }
    }

    fn exported_symbol_names(
        file: &IndexedFile,
        target_symbol_names: &HashSet<&str>,
    ) -> HashSet<String> {
        match file.language {
            LanguageId::Rust => {
                let content = String::from_utf8_lossy(&file.content);
                let mut exported = HashSet::new();
                for keyword in [
                    "fn", "struct", "enum", "trait", "type", "const", "static", "mod",
                ] {
                    for prefix in ["pub ", "pub(crate) ", "pub(super) ", "pub(self) "] {
                        let pattern = format!("{prefix}{keyword} ");
                        let mut start = 0usize;
                        while let Some(pos) = content[start..].find(&pattern) {
                            let ident_start = start + pos + pattern.len();
                            let ident_end = content[ident_start..]
                                .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
                                .map(|offset| ident_start + offset)
                                .unwrap_or(content.len());
                            if ident_end > ident_start {
                                let name = &content[ident_start..ident_end];
                                if target_symbol_names.contains(name) {
                                    exported.insert(name.to_string());
                                }
                            }
                            start = ident_start.saturating_add(1);
                        }
                    }
                }
                exported
            }
            LanguageId::JavaScript | LanguageId::TypeScript => target_symbol_names
                .iter()
                .copied()
                .filter(|name| Self::has_pub_symbol(file, name))
                .map(str::to_string)
                .collect(),
            _ => target_symbol_names
                .iter()
                .copied()
                .map(str::to_string)
                .collect(),
        }
    }

    /// Find all files that import (depend on) `target_path`.
    ///
    /// Uses two strategies:
    /// 1. **Stem matching** — the import's `name`/`qualified_name` contains the file stem
    ///    as a path segment (e.g. `import db` matches `src/db.rs`).
    /// 2. **Module path matching** — resolves the file to its logical module path
    ///    (e.g. `src/live_index/mod.rs` → `crate::live_index`) and checks if any import
    ///    starts with that module path. This handles `lib.rs`, `mod.rs`, `__init__.py`,
    ///    and `index.js`/`index.ts` which stem matching misses.
    ///
    /// Returns a `Vec` of `(importing_file_path, &import_reference)` tuples.
    pub fn find_dependents_for_file(&self, target_path: &str) -> Vec<(&str, &ReferenceRecord)> {
        let Some(target_file) = self.files.get(target_path) else {
            return vec![];
        };

        // Extract the file stem: "src/db.rs" → "db"
        let stem = std::path::Path::new(target_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(target_path);

        // Resolve the logical module path for the target file.
        let module_path = resolve_module_path(target_path, &target_file.language);

        let target_language = target_file.language.clone();
        let target_scope = declared_scope(target_file);
        let target_symbol_names: HashSet<&str> = target_file
            .symbols
            .iter()
            .map(|symbol| symbol.name.as_str())
            .filter(|name| !name.is_empty())
            .collect();
        let exported_target_symbol_names =
            Self::exported_symbol_names(target_file, &target_symbol_names);

        let mut results = Vec::new();

        for (file_path, file) in &self.files {
            // Don't report a file as depending on itself.
            if file_path.as_str() == target_path {
                continue;
            }

            let matching_imports: Vec<&ReferenceRecord> = file
                .references
                .iter()
                .filter(|reference| {
                    matches_target_import(
                        &file.language,
                        &target_language,
                        reference,
                        stem,
                        module_path.as_deref(),
                    )
                })
                .collect();

            if !target_symbol_names.is_empty()
                && (can_match_type_dependents(file, &target_language, target_scope.as_deref())
                    || !matching_imports.is_empty())
            {
                let symbol_refs: Vec<&ReferenceRecord> = file
                    .references
                    .iter()
                    .filter(|reference| {
                        reference.kind != ReferenceKind::Import
                            && target_symbol_names.contains(reference.name.as_str())
                            && exported_target_symbol_names.contains(reference.name.as_str())
                            && (reference.kind == ReferenceKind::TypeUsage
                                || (reference.kind == ReferenceKind::Call
                                    && (reference.qualified_name.as_deref().is_some_and(|qn| {
                                        matches_exact_symbol_qualified_name(
                                            &target_language,
                                            qn,
                                            &reference.name,
                                            module_path.as_deref(),
                                        )
                                    }) || (reference.qualified_name.is_none()
                                        && matching_imports
                                            .iter()
                                            .any(|import| import.name == reference.name)))))
                    })
                    .collect();

                if !symbol_refs.is_empty() {
                    results.extend(
                        symbol_refs
                            .into_iter()
                            .map(|reference| (file_path.as_str(), reference)),
                    );
                    continue;
                }
            }

            results.extend(
                matching_imports
                    .into_iter()
                    .map(|reference| (file_path.as_str(), reference)),
            );
        }

        // --- Qualified-call dependents (no import required) ---
        //
        // Files that use fully-qualified calls (e.g. `engine::optimize()`) without
        // a separate `use engine;` import are not caught by matching_imports above.
        // Scan for Call references whose qualified_name suffix-matches the target's
        // module path, so find_dependents stays consistent with find_references.
        if let Some(ref mp) = module_path {
            let already_found: HashSet<&str> = results.iter().map(|(p, _)| *p).collect();

            for (file_path, file) in &self.files {
                if file_path.as_str() == target_path || already_found.contains(file_path.as_str()) {
                    continue;
                }

                // Qualified calls resolve within the importer's own language; a
                // cross-language file cannot depend on this target via a call.
                if !import_languages_compatible(&file.language, &target_language) {
                    continue;
                }

                let qualified_refs: Vec<&ReferenceRecord> = file
                    .references
                    .iter()
                    .filter(|reference| {
                        reference.kind == ReferenceKind::Call
                            && reference.qualified_name.as_deref().is_some_and(|qn| {
                                // The qualified_name must refer to a symbol in the target file.
                                // Check: the ref's simple name is a public symbol in target,
                                // AND the qualified_name suffix-matches the module path.
                                target_symbol_names.contains(reference.name.as_str())
                                    && exported_target_symbol_names
                                        .contains(reference.name.as_str())
                                    && matches_exact_symbol_qualified_name(
                                        &target_language,
                                        qn,
                                        &reference.name,
                                        Some(mp),
                                    )
                            })
                    })
                    .collect();

                if !qualified_refs.is_empty() {
                    results.extend(
                        qualified_refs
                            .into_iter()
                            .map(|reference| (file_path.as_str(), reference)),
                    );
                }
            }
        }

        // --- Re-export chain resolution (Rust only, max 2 hops) ---
        //
        // If file X is re-exported via `pub use` in file Y, then files that
        // import from Y are also dependents of X.  We use a BFS with a depth
        // limit to avoid infinite loops and keep the search bounded.
        if target_language == LanguageId::Rust {
            let mut already_found: HashSet<&str> = results.iter().map(|(path, _)| *path).collect();

            let mut visited: HashSet<&str> = HashSet::new();
            visited.insert(target_path);

            // Seed: find files that pub-use-re-export from target
            let mut queue: VecDeque<(&str, u8)> = VecDeque::new();
            let reexporters = find_reexporters(
                &self.files,
                target_path,
                module_path.as_deref(),
                &target_language,
                stem,
            );
            for re_path in reexporters {
                if visited.insert(re_path) {
                    queue.push_back((re_path, 1));
                }
            }

            while let Some((reexporter_path, depth)) = queue.pop_front() {
                let Some(re_file) = self.files.get(reexporter_path) else {
                    continue;
                };

                let re_stem = std::path::Path::new(reexporter_path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or(reexporter_path);
                let re_module_path = resolve_module_path(reexporter_path, &re_file.language);

                // Find files that import from the re-exporter
                for (file_path, file) in &self.files {
                    if file_path.as_str() == target_path
                        || file_path.as_str() == reexporter_path
                        || already_found.contains(file_path.as_str())
                    {
                        continue;
                    }

                    let transitive_imports: Vec<&ReferenceRecord> = file
                        .references
                        .iter()
                        .filter(|reference| {
                            matches_target_import(
                                &file.language,
                                &target_language,
                                reference,
                                re_stem,
                                re_module_path.as_deref(),
                            )
                        })
                        .collect();

                    if !transitive_imports.is_empty() {
                        // Prefer symbol-level usage refs when target symbol names match
                        if !target_symbol_names.is_empty() {
                            let symbol_refs: Vec<&ReferenceRecord> = file
                                .references
                                .iter()
                                .filter(|reference| {
                                    reference.kind != ReferenceKind::Import
                                        && target_symbol_names.contains(reference.name.as_str())
                                        && exported_target_symbol_names
                                            .contains(reference.name.as_str())
                                })
                                .collect();
                            if !symbol_refs.is_empty() {
                                already_found.insert(file_path.as_str());
                                results.extend(
                                    symbol_refs.into_iter().map(|r| (file_path.as_str(), r)),
                                );
                                continue;
                            }
                        }

                        already_found.insert(file_path.as_str());
                        results.extend(
                            transitive_imports
                                .into_iter()
                                .map(|r| (file_path.as_str(), r)),
                        );
                    }
                }

                // Follow the chain one more hop if the re-exporter is itself
                // re-exported by another file (depth limit: 2).
                if depth < 2 {
                    let next_reexporters = find_reexporters(
                        &self.files,
                        reexporter_path,
                        re_module_path.as_deref(),
                        &target_language,
                        re_stem,
                    );
                    for next_path in next_reexporters {
                        if visited.insert(next_path) {
                            queue.push_back((next_path, depth + 1));
                        }
                    }
                }
            }
        }

        results.sort_by(|a, b| {
            a.0.cmp(b.0)
                .then(a.1.line_range.0.cmp(&b.1.line_range.0))
                .then(a.1.byte_range.0.cmp(&b.1.byte_range.0))
        });

        // Deduplicate (same file + same byte range = same reference)
        results.dedup_by(|a, b| a.0 == b.0 && a.1.byte_range == b.1.byte_range);

        results
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AdmissionTierLookupView, ContextBundleView, SearchFilesCouplingEvidence,
        SearchFilesCouplingNeighbors, SearchFilesHit, SearchFilesResolveView, SearchFilesTier,
        SearchFilesView,
    };
    use crate::domain::index::{AdmissionDecision, AdmissionTier, SkipReason, SkippedFile};
    use crate::domain::{LanguageId, ReferenceKind, ReferenceRecord, SymbolKind, SymbolRecord};
    use crate::live_index::store::{
        CircuitBreakerState, IndexState, IndexedFile, LiveIndex, ParseStatus,
    };
    use crate::watcher_state::{WatcherInfo, WatcherState};
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::{Duration, Instant, SystemTime};

    fn make_symbol(name: &str) -> SymbolRecord {
        let byte_range = (0, 10);
        SymbolRecord {
            name: name.to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range,
            item_byte_range: Some(byte_range),
            line_range: (0, 1),
            doc_byte_range: None,
        }
    }

    fn make_indexed_file(
        path: &str,
        symbols: Vec<SymbolRecord>,
        status: ParseStatus,
    ) -> IndexedFile {
        IndexedFile {
            relative_path: path.to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path(path),
            content: b"fn test() {}".to_vec(),
            symbols,
            parse_status: status,
            parse_diagnostic: None,
            byte_len: 12,
            content_hash: "abc".to_string(),
            references: vec![],
            alias_map: std::collections::HashMap::new(),
            mtime_secs: 0,
        }
    }

    fn make_indexed_file_with_language(
        path: &str,
        language: LanguageId,
        symbols: Vec<SymbolRecord>,
        status: ParseStatus,
    ) -> IndexedFile {
        let mut file = make_indexed_file(path, symbols, status);
        file.language = language;
        file
    }

    fn make_index(files: Vec<(&str, IndexedFile)>, tripped: bool) -> LiveIndex {
        let cb = CircuitBreakerState::new(0.20);
        if tripped {
            // Force-trip by recording enough failures
            for i in 0..10 {
                cb.record_success();
                if i < 7 {
                    cb.record_success();
                }
            }
            for i in 0..5 {
                cb.record_failure(&format!("f{i}.rs"), "error");
            }
            cb.should_abort();
        }

        let files_map: std::collections::HashMap<String, std::sync::Arc<IndexedFile>> = files
            .into_iter()
            .map(|(p, f)| (p.to_string(), std::sync::Arc::new(f)))
            .collect();
        let trigram_index = crate::live_index::trigram::TrigramIndex::build_from_files(&files_map);
        let mut index = LiveIndex {
            files: files_map,
            loaded_at: Instant::now(),
            loaded_at_system: std::time::SystemTime::now(),
            load_duration: Duration::from_millis(50),
            cb_state: cb,
            is_empty: false,
            load_source: crate::live_index::store::IndexLoadSource::FreshLoad,
            snapshot_verify_state: crate::live_index::store::SnapshotVerifyState::NotNeeded,
            reverse_index: std::collections::HashMap::new(),
            files_by_basename: std::collections::HashMap::new(),
            files_by_dir_component: std::collections::HashMap::new(),
            trigram_index,
            gitignore: None,
            skipped_files: Vec::new(),
            coupling_store: None,
            local_empty_reason: Arc::new(parking_lot::RwLock::new(None)),
            indexed_root: None,
        };
        // Rebuild the reverse index so xref query tests work.
        index.rebuild_reverse_index();
        index.rebuild_path_indices();
        index
    }

    // --- xref test helpers ---

    fn make_ref(
        name: &str,
        qualified_name: Option<&str>,
        kind: ReferenceKind,
        enclosing: Option<u32>,
        byte_start: u32,
    ) -> ReferenceRecord {
        ReferenceRecord {
            name: name.to_string(),
            qualified_name: qualified_name.map(|s| s.to_string()),
            kind,
            byte_range: (byte_start, byte_start + 10),
            line_range: (byte_start / 100, byte_start / 100),
            enclosing_symbol_index: enclosing,
        }
    }

    /// Build a `Call` reference with byte_range and line_range decoupled.
    ///
    /// `make_ref` hardcodes `line_range = (byte_start/100, byte_start/100)`, which
    /// couples the two and makes it impossible to land a ref both at a real byte
    /// offset (needed for the SF-002 byte-before-`.` receiver check) AND inside a
    /// method's line range (needed for the callee line-containment filter). This
    /// builder lets a test pass the exact byte offset of a call name together with
    /// the line it sits on.
    fn make_call_ref_lines(
        name: &str,
        byte_start: u32,
        line: u32,
        enclosing: Option<u32>,
    ) -> ReferenceRecord {
        ReferenceRecord {
            name: name.to_string(),
            qualified_name: None,
            kind: ReferenceKind::Call,
            byte_range: (byte_start, byte_start + name.len() as u32),
            line_range: (line, line),
            enclosing_symbol_index: enclosing,
        }
    }

    fn make_file_with_refs(
        path: &str,
        refs: Vec<ReferenceRecord>,
        alias_map: HashMap<String, String>,
    ) -> IndexedFile {
        IndexedFile {
            relative_path: path.to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path(path),
            content: b"fn test() {}".to_vec(),
            symbols: vec![],
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: 12,
            content_hash: "abc".to_string(),
            references: refs,
            alias_map,
            mtime_secs: 0,
        }
    }

    fn make_file_with_refs_and_content(
        path: &str,
        language: LanguageId,
        content: &str,
        refs: Vec<ReferenceRecord>,
        symbols: Vec<SymbolRecord>,
    ) -> IndexedFile {
        IndexedFile {
            relative_path: path.to_string(),
            language,
            classification: crate::domain::FileClassification::for_code_path(path),
            content: content.as_bytes().to_vec(),
            symbols,
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: content.len() as u64,
            content_hash: "abc".to_string(),
            references: refs,
            alias_map: HashMap::new(),
            mtime_secs: 0,
        }
    }

    fn make_symbol_with_kind_and_line(name: &str, kind: SymbolKind, line: u32) -> SymbolRecord {
        let byte_range = (0, 10);
        SymbolRecord {
            name: name.to_string(),
            kind,
            depth: 0,
            sort_order: 0,
            byte_range,
            item_byte_range: Some(byte_range),
            line_range: (line, line),
            doc_byte_range: None,
        }
    }

    fn make_symbol_with_kind_line_and_bytes(
        name: &str,
        kind: SymbolKind,
        line: u32,
        byte_range: (u32, u32),
    ) -> SymbolRecord {
        SymbolRecord {
            name: name.to_string(),
            kind,
            depth: 0,
            sort_order: 0,
            byte_range,
            item_byte_range: Some(byte_range),
            line_range: (line, line),
            doc_byte_range: None,
        }
    }

    #[test]
    fn test_get_file_returns_some_for_existing() {
        let f = make_indexed_file(
            "src/main.rs",
            vec![make_symbol("main")],
            ParseStatus::Parsed,
        );
        let index = make_index(vec![("src/main.rs", f)], false);
        assert!(index.get_file("src/main.rs").is_some());
    }

    #[test]
    fn test_has_pub_symbol_recognizes_restricted_pub_visibilities() {
        let mut file = make_indexed_file("src/lib.rs", vec![], ParseStatus::Parsed);
        file.content =
            b"pub(super) fn restricted_fn() {}\npub(self) struct Inner {}\nfn private_fn() {}"
                .to_vec();
        assert!(
            super::LiveIndex::has_pub_symbol(&file, "restricted_fn"),
            "pub(super) fn must count as an exported symbol"
        );
        assert!(
            super::LiveIndex::has_pub_symbol(&file, "Inner"),
            "pub(self) struct must count as an exported symbol"
        );
        assert!(
            !super::LiveIndex::has_pub_symbol(&file, "private_fn"),
            "a non-pub fn must not count as exported"
        );
    }

    #[test]
    fn test_get_file_returns_none_for_missing() {
        let index = make_index(vec![], false);
        assert!(index.get_file("nonexistent.rs").is_none());
    }

    #[test]
    fn test_symbols_for_file_returns_slice() {
        let sym = make_symbol("foo");
        let f = make_indexed_file("src/main.rs", vec![sym.clone()], ParseStatus::Parsed);
        let index = make_index(vec![("src/main.rs", f)], false);
        let syms = index.symbols_for_file("src/main.rs");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "foo");
    }

    #[test]
    fn test_symbols_for_file_returns_empty_for_missing() {
        let index = make_index(vec![], false);
        let syms = index.symbols_for_file("nonexistent.rs");
        assert!(syms.is_empty());
    }

    #[test]
    fn test_all_files_returns_all_entries() {
        let f1 = make_indexed_file("a.rs", vec![], ParseStatus::Parsed);
        let f2 = make_indexed_file("b.rs", vec![], ParseStatus::Parsed);
        let index = make_index(vec![("a.rs", f1), ("b.rs", f2)], false);
        let pairs: Vec<_> = index.all_files().collect();
        assert_eq!(pairs.len(), 2);
    }

    #[test]
    fn test_capture_admission_tier_lookup_view_returns_tier1_indexed_metadata() {
        let file = make_indexed_file(
            "src/main.rs",
            vec![make_symbol("main")],
            ParseStatus::Parsed,
        );
        let index = make_index(vec![("src/main.rs", file)], false);

        assert_eq!(
            index.capture_admission_tier_lookup_view("./src\\main.rs"),
            Some(AdmissionTierLookupView {
                tier: AdmissionTier::Normal,
                path: "src/main.rs".to_string(),
                size: Some(12),
                extension: None,
                language: Some(LanguageId::Rust),
                reason: None,
            })
        );
    }

    #[test]
    fn test_capture_admission_tier_lookup_view_returns_tier2_skipped_metadata() {
        let mut index = make_index(vec![], false);
        index.add_skipped_file(SkippedFile {
            path: "models/v1.safetensors".to_string(),
            size: 4096,
            extension: Some("safetensors".to_string()),
            decision: AdmissionDecision::skip(
                AdmissionTier::MetadataOnly,
                SkipReason::DenylistedExtension,
            ),
        });

        assert_eq!(
            index.capture_admission_tier_lookup_view("models\\v1.safetensors"),
            Some(AdmissionTierLookupView {
                tier: AdmissionTier::MetadataOnly,
                path: "models/v1.safetensors".to_string(),
                size: Some(4096),
                extension: Some("safetensors".to_string()),
                language: None,
                reason: Some(SkipReason::DenylistedExtension),
            })
        );
    }

    #[test]
    fn test_capture_admission_tier_lookup_view_returns_tier3_reason() {
        let mut index = make_index(vec![], false);
        index.add_skipped_file(SkippedFile {
            path: "artifacts/huge.bin".to_string(),
            size: 150 * 1024 * 1024,
            extension: Some("bin".to_string()),
            decision: AdmissionDecision::skip(AdmissionTier::HardSkip, SkipReason::SizeCeiling),
        });

        assert_eq!(
            index.capture_admission_tier_lookup_view("./artifacts\\huge.bin"),
            Some(AdmissionTierLookupView {
                tier: AdmissionTier::HardSkip,
                path: "artifacts/huge.bin".to_string(),
                size: Some(150 * 1024 * 1024),
                extension: Some("bin".to_string()),
                language: None,
                reason: Some(SkipReason::SizeCeiling),
            })
        );
    }

    #[test]
    fn test_capture_repo_outline_view_sorts_paths_and_counts_symbols() {
        let f1 = make_indexed_file(
            "src/zeta.rs",
            vec![make_symbol("zeta"), make_symbol("helper")],
            ParseStatus::Parsed,
        );
        let f2 = make_indexed_file(
            "src/alpha.rs",
            vec![make_symbol("alpha")],
            ParseStatus::Parsed,
        );
        let index = make_index(vec![("src/zeta.rs", f1), ("src/alpha.rs", f2)], false);

        let view = index.capture_repo_outline_view();

        assert_eq!(view.total_files, 2);
        assert_eq!(view.total_symbols, 3);
        assert_eq!(
            view.files
                .iter()
                .map(|file| file.relative_path.as_str())
                .collect::<Vec<_>>(),
            vec!["src/alpha.rs", "src/zeta.rs"]
        );
        assert_eq!(view.files[0].symbol_count, 1);
        assert_eq!(view.files[1].symbol_count, 2);
    }

    #[test]
    fn test_capture_file_outline_view_clones_path_and_symbols() {
        let f = make_indexed_file(
            "src/main.rs",
            vec![make_symbol("main"), make_symbol("helper")],
            ParseStatus::Parsed,
        );
        let index = make_index(vec![("src/main.rs", f)], false);

        let view = index
            .capture_file_outline_view("src/main.rs")
            .expect("captured outline view");

        assert_eq!(view.relative_path, "src/main.rs");
        assert_eq!(
            view.symbols
                .iter()
                .map(|symbol| symbol.name.as_str())
                .collect::<Vec<_>>(),
            vec!["main", "helper"]
        );
    }

    #[test]
    fn test_capture_symbol_detail_view_clones_content_and_symbols() {
        let f = make_indexed_file(
            "src/lib.rs",
            vec![make_symbol("foo"), make_symbol("bar")],
            ParseStatus::Parsed,
        );
        let index = make_index(vec![("src/lib.rs", f)], false);

        let view = index
            .capture_symbol_detail_view("src/lib.rs")
            .expect("captured symbol detail view");

        assert_eq!(view.relative_path, "src/lib.rs");
        assert_eq!(view.content, b"fn test() {}".to_vec());
        assert_eq!(
            view.symbols
                .iter()
                .map(|symbol| symbol.name.as_str())
                .collect::<Vec<_>>(),
            vec!["foo", "bar"]
        );
    }

    #[test]
    fn test_capture_file_content_view_clones_path_and_content() {
        let f = make_indexed_file("src/lib.rs", vec![], ParseStatus::Parsed);
        let index = make_index(vec![("src/lib.rs", f)], false);

        let view = index
            .capture_file_content_view("src/lib.rs")
            .expect("captured file content view");

        assert_eq!(view.relative_path, "src/lib.rs");
        assert_eq!(view.content, b"fn test() {}".to_vec());
    }

    #[test]
    fn test_capture_shared_file_returns_same_arc_entry() {
        let f = make_indexed_file("src/lib.rs", vec![], ParseStatus::Parsed);
        let index = make_index(vec![("src/lib.rs", f)], false);

        let shared = index
            .capture_shared_file("src/lib.rs")
            .expect("captured shared file");
        let stored = index.files.get("src/lib.rs").expect("stored file");

        assert!(std::sync::Arc::ptr_eq(stored, &shared));
        assert_eq!(shared.relative_path, "src/lib.rs");
        assert_eq!(shared.content, b"fn test() {}".to_vec());
    }

    #[test]
    fn test_capture_shared_file_for_scope_exact_returns_same_arc_entry() {
        let f = make_indexed_file("src/lib.rs", vec![], ParseStatus::Parsed);
        let index = make_index(vec![("src/lib.rs", f)], false);

        let shared = index
            .capture_shared_file_for_scope(&super::PathScope::exact("src/lib.rs"))
            .expect("captured scoped shared file");
        let stored = index.files.get("src/lib.rs").expect("stored file");

        assert!(std::sync::Arc::ptr_eq(stored, &shared));
    }

    #[test]
    fn test_capture_shared_file_for_scope_unique_prefix_returns_only_match() {
        let index = make_index(
            vec![
                (
                    "src/lib.rs",
                    make_indexed_file("src/lib.rs", vec![], ParseStatus::Parsed),
                ),
                (
                    "src/main.rs",
                    make_indexed_file("src/main.rs", vec![], ParseStatus::Parsed),
                ),
                (
                    "tests/lib_test.rs",
                    make_indexed_file("tests/lib_test.rs", vec![], ParseStatus::Parsed),
                ),
            ],
            false,
        );

        let unique = index
            .capture_shared_file_for_scope(&super::PathScope::prefix("tests/"))
            .expect("unique prefix should resolve");
        assert_eq!(unique.relative_path, "tests/lib_test.rs");

        // When multiple files match a prefix, the function returns the shortest
        // path (sorted by length). "src/lib.rs" (10 chars) < "src/main.rs" (11).
        let multi_match = index
            .capture_shared_file_for_scope(&super::PathScope::prefix("src/"))
            .expect("prefix with multiple matches should return shortest path");
        assert_eq!(multi_match.relative_path, "src/lib.rs");

        let any = index.capture_shared_file_for_scope(&super::PathScope::any());
        assert!(any.is_none(), "Any scope should not guess a single file");
    }

    #[test]
    fn test_capture_what_changed_timestamp_view_sorts_paths() {
        let f1 = make_indexed_file("src/z.rs", vec![], ParseStatus::Parsed);
        let f2 = make_indexed_file("src/a.rs", vec![], ParseStatus::Parsed);
        let index = make_index(vec![("src/z.rs", f1), ("src/a.rs", f2)], false);

        let view = index.capture_what_changed_timestamp_view();

        assert!(view.loaded_secs >= 0);
        assert_eq!(
            view.paths,
            vec!["src/a.rs".to_string(), "src/z.rs".to_string()]
        );
    }

    #[test]
    fn test_capture_search_files_resolve_view_returns_exact_path_match() {
        let index = make_index(
            vec![(
                "src/protocol/tools.rs",
                make_indexed_file("src/protocol/tools.rs", vec![], ParseStatus::Parsed),
            )],
            false,
        );

        let view = index.capture_search_files_resolve_view("./src\\protocol\\tools.rs");

        assert_eq!(
            view,
            SearchFilesResolveView::Resolved {
                path: "src/protocol/tools.rs".to_string()
            }
        );
    }

    #[test]
    fn test_capture_search_files_resolve_view_uses_basename_and_dir_component_narrowing() {
        let index = make_index(
            vec![
                (
                    "src/protocol/tools.rs",
                    make_indexed_file("src/protocol/tools.rs", vec![], ParseStatus::Parsed),
                ),
                (
                    "src/sidecar/tools.rs",
                    make_indexed_file("src/sidecar/tools.rs", vec![], ParseStatus::Parsed),
                ),
            ],
            false,
        );

        let view = index.capture_search_files_resolve_view("protocol/tools.rs");

        assert_eq!(
            view,
            SearchFilesResolveView::Resolved {
                path: "src/protocol/tools.rs".to_string()
            }
        );
    }

    #[test]
    fn test_capture_search_files_resolve_view_falls_back_to_partial_path_match() {
        let index = make_index(
            vec![(
                "src/protocol/tools.rs",
                make_indexed_file("src/protocol/tools.rs", vec![], ParseStatus::Parsed),
            )],
            false,
        );

        let view = index.capture_search_files_resolve_view("protocol/tools");

        assert_eq!(
            view,
            SearchFilesResolveView::Resolved {
                path: "src/protocol/tools.rs".to_string()
            }
        );
    }

    #[test]
    fn test_capture_search_files_resolve_view_returns_bounded_ambiguous_matches() {
        let index = make_index(
            vec![
                (
                    "src/lib.rs",
                    make_indexed_file("src/lib.rs", vec![], ParseStatus::Parsed),
                ),
                (
                    "tests/lib.rs",
                    make_indexed_file("tests/lib.rs", vec![], ParseStatus::Parsed),
                ),
            ],
            false,
        );

        let view = index.capture_search_files_resolve_view("lib.rs");

        assert_eq!(
            view,
            SearchFilesResolveView::Ambiguous {
                hint: "lib.rs".to_string(),
                matches: vec!["src/lib.rs".to_string(), "tests/lib.rs".to_string()],
                overflow_count: 0,
            }
        );
    }

    #[test]
    fn test_capture_search_files_view_groups_tiers_and_caps_results() {
        let index = make_index(
            vec![
                (
                    "src/protocol/tools.rs",
                    make_indexed_file("src/protocol/tools.rs", vec![], ParseStatus::Parsed),
                ),
                (
                    "src/sidecar/tools.rs",
                    make_indexed_file("src/sidecar/tools.rs", vec![], ParseStatus::Parsed),
                ),
                (
                    "src/protocol/tools_helper.rs",
                    make_indexed_file("src/protocol/tools_helper.rs", vec![], ParseStatus::Parsed),
                ),
            ],
            false,
        );

        let view = index.capture_search_files_view("protocol/tools.rs", 2, None, None);

        assert_eq!(
            view,
            SearchFilesView::Found {
                query: "protocol/tools.rs".to_string(),
                total_matches: 2,
                overflow_count: 0,
                hits: vec![
                    SearchFilesHit {
                        tier: SearchFilesTier::StrongPath,
                        path: "src/protocol/tools.rs".to_string(),
                        coupling_score: None,
                        shared_commits: None,
                    },
                    SearchFilesHit {
                        tier: SearchFilesTier::Basename,
                        path: "src/sidecar/tools.rs".to_string(),
                        coupling_score: None,
                        shared_commits: None,
                    },
                ],
            }
        );
    }

    #[test]
    fn test_capture_search_files_view_returns_loose_path_matches_for_component_query() {
        let index = make_index(
            vec![
                (
                    "src/live_index/query.rs",
                    make_indexed_file("src/live_index/query.rs", vec![], ParseStatus::Parsed),
                ),
                (
                    "src/live_index/store.rs",
                    make_indexed_file("src/live_index/store.rs", vec![], ParseStatus::Parsed),
                ),
            ],
            false,
        );

        let view = index.capture_search_files_view("live_index", 20, None, None);

        assert_eq!(
            view,
            SearchFilesView::Found {
                query: "live_index".to_string(),
                total_matches: 2,
                overflow_count: 0,
                hits: vec![
                    SearchFilesHit {
                        tier: SearchFilesTier::LoosePath,
                        path: "src/live_index/query.rs".to_string(),
                        coupling_score: None,
                        shared_commits: None,
                    },
                    SearchFilesHit {
                        tier: SearchFilesTier::LoosePath,
                        path: "src/live_index/store.rs".to_string(),
                        coupling_score: None,
                        shared_commits: None,
                    },
                ],
            }
        );
    }

    #[test]
    fn test_capture_search_files_view_prefix_matching() {
        let index = make_index(
            vec![
                (
                    "src/orchestrator.rs",
                    make_indexed_file("src/orchestrator.rs", vec![], ParseStatus::Parsed),
                ),
                (
                    "src/orchestration.rs",
                    make_indexed_file("src/orchestration.rs", vec![], ParseStatus::Parsed),
                ),
                (
                    "src/other.rs",
                    make_indexed_file("src/other.rs", vec![], ParseStatus::Parsed),
                ),
            ],
            false,
        );

        // "orchestrat" should find both orchestrator.rs and orchestration.rs via prefix matching.
        let view = index.capture_search_files_view("orchestrat", 10, None, None);
        if let SearchFilesView::Found { hits, .. } = view {
            let paths: Vec<&str> = hits.iter().map(|h| h.path.as_str()).collect();
            assert!(
                paths.contains(&"src/orchestrator.rs"),
                "expected orchestrator.rs in results: {paths:?}"
            );
            assert!(
                paths.contains(&"src/orchestration.rs"),
                "expected orchestration.rs in results: {paths:?}"
            );
            assert!(
                !paths.contains(&"src/other.rs"),
                "unexpected other.rs in results: {paths:?}"
            );
        } else {
            panic!("expected found view for prefix query 'orchestrat'");
        }
    }

    #[test]
    fn test_capture_search_files_view_boosts_local_results() {
        let index = make_index(
            vec![
                (
                    "src/client/utils.rs",
                    make_indexed_file("src/client/utils.rs", vec![], ParseStatus::Parsed),
                ),
                (
                    "src/server/utils.rs",
                    make_indexed_file("src/server/utils.rs", vec![], ParseStatus::Parsed),
                ),
            ],
            false,
        );

        // When in server context, server utils should rank first.
        let view_server =
            index.capture_search_files_view("utils.rs", 10, Some("src/server/main.rs"), None);
        if let SearchFilesView::Found { hits, .. } = view_server {
            assert_eq!(hits[0].path, "src/server/utils.rs");
        } else {
            panic!("expected found view");
        }

        // When in client context, client utils should rank first.
        let view_client =
            index.capture_search_files_view("utils.rs", 10, Some("src/client/main.rs"), None);
        if let SearchFilesView::Found { hits, .. } = view_client {
            assert_eq!(hits[0].path, "src/client/utils.rs");
        } else {
            panic!("expected found view");
        }
    }

    #[test]
    fn test_capture_search_files_with_coupling_filters_obsidian_internals() {
        let index = make_index(
            vec![
                (
                    "wiki/notes.md",
                    make_indexed_file("wiki/notes.md", vec![], ParseStatus::Parsed),
                ),
                (
                    "wiki/.obsidian/notes.md",
                    make_indexed_file("wiki/.obsidian/notes.md", vec![], ParseStatus::Parsed),
                ),
                (
                    ".obsidian/plugins/dataview/styles.css",
                    make_indexed_file(
                        ".obsidian/plugins/dataview/styles.css",
                        vec![],
                        ParseStatus::Parsed,
                    ),
                ),
            ],
            false,
        );
        let mut neighbors = SearchFilesCouplingNeighbors::new();
        neighbors.insert(
            "wiki/notes.md".to_string(),
            SearchFilesCouplingEvidence {
                shared_commits: 3,
                weighted_score: 3.0,
            },
        );
        neighbors.insert(
            "wiki/.obsidian/notes.md".to_string(),
            SearchFilesCouplingEvidence {
                shared_commits: 9,
                weighted_score: 9.0,
            },
        );

        let view = index.capture_search_files_view_with_noise(
            "notes.md",
            10,
            None,
            Some(("wiki/notes.md", &neighbors)),
            true,
            false,
        );

        assert_eq!(
            view,
            SearchFilesView::Found {
                query: "notes.md".to_string(),
                total_matches: 1,
                overflow_count: 0,
                hits: vec![SearchFilesHit {
                    tier: SearchFilesTier::CoChange,
                    path: "wiki/notes.md".to_string(),
                    coupling_score: Some(1.0),
                    shared_commits: Some(3),
                }],
            }
        );
    }

    #[test]
    fn test_capture_find_dependents_view_groups_files_and_lines() {
        let target = make_file_with_refs_and_content(
            "src/db.rs",
            LanguageId::Rust,
            "pub fn connect() {}\n",
            vec![],
            vec![SymbolRecord {
                name: "connect".to_string(),
                kind: SymbolKind::Function,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 18),
                line_range: (0, 0),
                doc_byte_range: None,
                item_byte_range: None,
            }],
        );
        let importer = make_file_with_refs_and_content(
            "src/app.rs",
            LanguageId::Rust,
            "use crate::db;\nfn run() { db::connect(); }\n",
            vec![
                make_ref("db", Some("crate::db"), ReferenceKind::Import, None, 0),
                make_ref(
                    "connect",
                    Some("db::connect"),
                    ReferenceKind::Call,
                    Some(0),
                    100,
                ),
            ],
            vec![SymbolRecord {
                name: "run".to_string(),
                kind: SymbolKind::Function,
                depth: 0,
                sort_order: 0,
                byte_range: (15, 38),
                line_range: (1, 1),
                doc_byte_range: None,
                item_byte_range: None,
            }],
        );
        let index = make_index(vec![("src/db.rs", target), ("src/app.rs", importer)], false);

        let view = index.capture_find_dependents_view("src/db.rs");

        assert_eq!(view.files.len(), 1);
        assert_eq!(view.files[0].file_path, "src/app.rs");
        assert_eq!(view.files[0].lines[0].line_number, 2);
        assert!(view.files[0].lines[0].line_content.contains("connect"));
    }

    #[test]
    fn test_capture_find_references_view_groups_context_lines() {
        let target = make_file_with_refs_and_content(
            "src/lib.rs",
            LanguageId::Rust,
            "fn process() {}\n",
            vec![],
            vec![SymbolRecord {
                name: "process".to_string(),
                kind: SymbolKind::Function,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 14),
                line_range: (0, 0),
                doc_byte_range: None,
                item_byte_range: None,
            }],
        );
        let caller = make_file_with_refs_and_content(
            "src/app.rs",
            LanguageId::Rust,
            "fn run() {\n    process();\n}\n",
            vec![make_ref("process", None, ReferenceKind::Call, Some(0), 100)],
            vec![SymbolRecord {
                name: "run".to_string(),
                kind: SymbolKind::Function,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 28),
                line_range: (0, 2),
                doc_byte_range: None,
                item_byte_range: None,
            }],
        );
        let index = make_index(vec![("src/lib.rs", target), ("src/app.rs", caller)], false);

        let view = index.capture_find_references_view("process", Some("call"), 200);

        assert_eq!(view.total_refs, 1);
        assert_eq!(view.files.len(), 1);
        assert_eq!(view.files[0].file_path, "src/app.rs");
        assert_eq!(view.files[0].hits[0].context_lines[1].line_number, 2);
        assert!(
            view.files[0].hits[0].context_lines[1]
                .text
                .contains("process")
        );
        assert!(
            view.files[0].hits[0].context_lines[1]
                .enclosing_annotation
                .as_deref()
                .unwrap_or("")
                .contains("run")
        );
    }

    #[test]
    fn test_capture_context_bundle_view_collects_owned_sections() {
        let target = make_file_with_refs_and_content(
            "src/db.rs",
            LanguageId::Rust,
            "fn process() {\n    helper();\n}\nfn helper() {}\n",
            vec![make_ref("helper", None, ReferenceKind::Call, Some(0), 100)],
            vec![
                SymbolRecord {
                    name: "process".to_string(),
                    kind: SymbolKind::Function,
                    depth: 0,
                    sort_order: 0,
                    byte_range: (0, 30),
                    line_range: (0, 2),
                    doc_byte_range: None,
                    item_byte_range: None,
                },
                SymbolRecord {
                    name: "helper".to_string(),
                    kind: SymbolKind::Function,
                    depth: 0,
                    sort_order: 1,
                    byte_range: (31, 45),
                    line_range: (3, 3),
                    doc_byte_range: None,
                    item_byte_range: None,
                },
            ],
        );
        let caller = make_file_with_refs_and_content(
            "src/app.rs",
            LanguageId::Rust,
            "use crate::db::process;\nfn run() {\n    process();\n    let value: process;\n}\n",
            vec![
                make_ref("db", Some("crate::db"), ReferenceKind::Import, Some(0), 0),
                make_ref(
                    "process",
                    Some("crate::db::process"),
                    ReferenceKind::Call,
                    Some(0),
                    100,
                ),
                make_ref(
                    "process",
                    Some("crate::db::process"),
                    ReferenceKind::TypeUsage,
                    Some(0),
                    200,
                ),
            ],
            vec![SymbolRecord {
                name: "run".to_string(),
                kind: SymbolKind::Function,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 52),
                line_range: (0, 3),
                doc_byte_range: None,
                item_byte_range: None,
            }],
        );
        let index = make_index(vec![("src/db.rs", target), ("src/app.rs", caller)], false);

        let view = index.capture_context_bundle_view("src/db.rs", "process", None, None);

        let super::ContextBundleView::Found(found) = view else {
            panic!("expected found context bundle view");
        };

        assert!(found.body.contains("fn process"));
        assert_eq!(found.kind_label, "fn");
        assert_eq!(found.callers.total_count, 1);
        assert_eq!(found.callers.entries[0].file_path, "src/app.rs");
        assert_eq!(found.callers.entries[0].line_number, 2);
        assert!(
            found.callers.entries[0]
                .enclosing
                .as_deref()
                .unwrap_or("")
                .contains("run")
        );
        assert_eq!(found.callees.total_count, 1);
        assert_eq!(found.callees.entries[0].display_name, "helper");
        assert_eq!(found.type_usages.total_count, 1);
    }

    #[test]
    fn test_capture_context_bundle_view_requires_line_for_ambiguous_selector() {
        let content = "fn connect() { first(); }\nfn connect() { second(); }\n";
        let first_body = "fn connect() { first(); }";
        let second_body = "fn connect() { second(); }";
        let second_start = content.find(second_body).unwrap() as u32;
        let target = make_file_with_refs_and_content(
            "src/db.rs",
            LanguageId::Rust,
            content,
            vec![],
            vec![
                make_symbol_with_kind_line_and_bytes(
                    "connect",
                    SymbolKind::Function,
                    1,
                    (0, first_body.len() as u32),
                ),
                make_symbol_with_kind_line_and_bytes(
                    "connect",
                    SymbolKind::Function,
                    2,
                    (second_start, second_start + second_body.len() as u32),
                ),
            ],
        );
        let index = make_index(vec![("src/db.rs", target)], false);

        let view = index.capture_context_bundle_view("src/db.rs", "connect", Some("fn"), None);

        match view {
            ContextBundleView::AmbiguousSymbol {
                path,
                name,
                candidate_lines,
            } => {
                assert_eq!(path, "src/db.rs");
                assert_eq!(name, "connect");
                assert_eq!(candidate_lines, vec![1, 2]);
            }
            _ => panic!("expected ambiguous selector"),
        }
    }

    #[test]
    fn test_capture_context_bundle_view_uses_symbol_line_and_exact_callers() {
        let content = "fn connect() { first(); }\nfn connect() { second(); }\n";
        let first_body = "fn connect() { first(); }";
        let second_body = "fn connect() { second(); }";
        let second_start = content.find(second_body).unwrap() as u32;
        let target = make_file_with_refs_and_content(
            "src/db.rs",
            LanguageId::Rust,
            content,
            vec![],
            vec![
                make_symbol_with_kind_line_and_bytes(
                    "connect",
                    SymbolKind::Function,
                    1,
                    (0, first_body.len() as u32),
                ),
                make_symbol_with_kind_line_and_bytes(
                    "connect",
                    SymbolKind::Function,
                    2,
                    (second_start, second_start + second_body.len() as u32),
                ),
            ],
        );
        let dependent = make_file_with_refs_and_content(
            "src/service.rs",
            LanguageId::Rust,
            "use crate::db::connect;\nfn run() { connect(); }\n",
            vec![
                make_ref("db", Some("crate::db"), ReferenceKind::Import, Some(0), 0),
                make_ref(
                    "connect",
                    Some("crate::db::connect"),
                    ReferenceKind::Call,
                    Some(0),
                    100,
                ),
            ],
            vec![make_symbol("run")],
        );
        let unrelated = make_file_with_refs_and_content(
            "src/other.rs",
            LanguageId::Rust,
            "fn run() { connect(); }\n",
            vec![make_ref("connect", None, ReferenceKind::Call, Some(0), 0)],
            vec![make_symbol("run")],
        );
        let index = make_index(
            vec![
                ("src/db.rs", target),
                ("src/service.rs", dependent),
                ("src/other.rs", unrelated),
            ],
            false,
        );

        let view = index.capture_context_bundle_view("src/db.rs", "connect", Some("fn"), Some(3));

        let ContextBundleView::Found(found) = view else {
            panic!("expected found view");
        };

        assert!(found.body.contains("second();"), "got: {}", found.body);
        assert!(!found.body.contains("first();"), "got: {}", found.body);
        assert_eq!(found.callers.total_count, 1);
        assert_eq!(found.callers.entries.len(), 1);
        assert_eq!(found.callers.entries[0].file_path, "src/service.rs");
    }

    #[test]
    fn test_sf002_ts_same_name_member_call_not_self_caller_or_callee() {
        // SF-002: a TS method whose body delegates to a same-named method on
        // another object (`this.testingService.startExploration()`) must NOT be
        // reported as its own caller or its own callee. The cross-object call is
        // surfaced in the unresolved-same-name-member-call section instead.
        let line0 = "class TestingService { startExploration() {} }";
        let line1 = "class TestingController { startExploration() { return this.testingService.startExploration(); } }";
        let content = format!("{line0}\n{line1}\n");

        let service_method_byte = content.find("startExploration() {}").unwrap() as u32;
        let controller_method_byte = content.find("startExploration() { return").unwrap() as u32;
        // The delegated member call: the `startExploration` that follows a `.`.
        let member_call_byte = content.find("testingService.startExploration()").unwrap() as u32
            + "testingService.".len() as u32;
        // Sanity: the byte immediately before the member call must be a `.`.
        assert_eq!(content.as_bytes()[member_call_byte as usize - 1], b'.');

        let target = make_file_with_refs_and_content(
            "src/testing.ts",
            LanguageId::TypeScript,
            &content,
            vec![make_call_ref_lines(
                "startExploration",
                member_call_byte,
                1,
                // Enclosed by the controller method (symbol index 1).
                Some(1),
            )],
            vec![
                // Index 0: service method on line 0.
                make_symbol_with_kind_line_and_bytes(
                    "startExploration",
                    SymbolKind::Method,
                    0,
                    (service_method_byte, service_method_byte + 20),
                ),
                // Index 1: controller method on line 1 (the target).
                make_symbol_with_kind_line_and_bytes(
                    "startExploration",
                    SymbolKind::Method,
                    1,
                    (
                        controller_method_byte,
                        controller_method_byte
                            + "startExploration() { return this.testingService.startExploration(); }"
                                .len() as u32,
                    ),
                ),
            ],
        );
        let index = make_index(vec![("src/testing.ts", target)], false);

        let view = index.capture_context_bundle_view(
            "src/testing.ts",
            "startExploration",
            Some("fn"),
            Some(2),
        );

        let ContextBundleView::Found(found) = view else {
            panic!("expected found view");
        };

        // (1) The controller method is NOT its own caller.
        assert_eq!(
            found.callers.total_count, 0,
            "self member-call must not be counted as a caller; got {:?}",
            found.callers.entries
        );
        // (2) The controller method is NOT its own callee.
        assert!(
            found
                .callees
                .entries
                .iter()
                .all(|e| e.display_name != "startExploration"),
            "self member-call must not be rendered as a callee; got {:?}",
            found.callees.entries
        );
        // (3) The cross-object same-name call is surfaced separately.
        assert_eq!(
            found.unresolved_same_name_member_calls.len(),
            1,
            "expected the unresolved member call to be surfaced; got {:?}",
            found.unresolved_same_name_member_calls
        );
        assert_eq!(
            found.unresolved_same_name_member_calls[0].display_name,
            "startExploration"
        );
    }

    #[test]
    fn test_sf002_csharp_same_name_member_call_not_self_caller_or_callee() {
        // C# parallel: `this.testingService.StartExploration()` from inside
        // `Controller.StartExploration` must not be self-caller/self-callee.
        let line0 = "class TestingService { public void StartExploration() {} }";
        let line1 = "class TestingController { public void StartExploration() { this.testingService.StartExploration(); } }";
        let content = format!("{line0}\n{line1}\n");

        let service_method_byte = content.find("StartExploration() {}").unwrap() as u32;
        let controller_method_byte = content.find("StartExploration() { this").unwrap() as u32;
        let member_call_byte = content.find("testingService.StartExploration()").unwrap() as u32
            + "testingService.".len() as u32;
        assert_eq!(content.as_bytes()[member_call_byte as usize - 1], b'.');

        let target = make_file_with_refs_and_content(
            "src/Testing.cs",
            LanguageId::CSharp,
            &content,
            vec![make_call_ref_lines(
                "StartExploration",
                member_call_byte,
                1,
                Some(1),
            )],
            vec![
                make_symbol_with_kind_line_and_bytes(
                    "StartExploration",
                    SymbolKind::Method,
                    0,
                    (service_method_byte, service_method_byte + 20),
                ),
                make_symbol_with_kind_line_and_bytes(
                    "StartExploration",
                    SymbolKind::Method,
                    1,
                    (
                        controller_method_byte,
                        controller_method_byte
                            + "StartExploration() { this.testingService.StartExploration(); }".len()
                                as u32,
                    ),
                ),
            ],
        );
        let index = make_index(vec![("src/Testing.cs", target)], false);

        let view = index.capture_context_bundle_view(
            "src/Testing.cs",
            "StartExploration",
            Some("fn"),
            Some(2),
        );

        let ContextBundleView::Found(found) = view else {
            panic!("expected found view");
        };

        assert_eq!(found.callers.total_count, 0);
        assert!(
            found
                .callees
                .entries
                .iter()
                .all(|e| e.display_name != "StartExploration")
        );
        assert_eq!(found.unresolved_same_name_member_calls.len(), 1);
    }

    #[test]
    fn test_sf002_bare_top_level_call_still_counted_as_caller() {
        // Guard against over-broadening: a true bare `foo()` call (no preceding
        // `.`) from a SIBLING symbol must still be counted as a caller, and a bare
        // intra-body recursive `foo()` must NOT be suppressed by the receiver guard.
        let line0 = "fn target() { target(); }";
        let line1 = "fn sibling() { target(); }";
        let content = format!("{line0}\n{line1}\n");

        let target_byte = content.find("fn target").unwrap() as u32;
        let sibling_byte = content.find("fn sibling").unwrap() as u32;
        // The recursive call inside target's body: the `target` after `{ `.
        let recursive_call_byte = content.find("{ target()").unwrap() as u32 + "{ ".len() as u32;
        // The sibling's call to target.
        let sibling_call_byte = content.find("fn sibling() { target()").unwrap() as u32
            + "fn sibling() { ".len() as u32;
        // Neither call is a receiver method call (no preceding `.`).
        assert_ne!(content.as_bytes()[recursive_call_byte as usize - 1], b'.');
        assert_ne!(content.as_bytes()[sibling_call_byte as usize - 1], b'.');

        let target = make_file_with_refs_and_content(
            "src/recur.rs",
            LanguageId::Rust,
            &content,
            vec![
                // Recursive bare call inside target (enclosing = target, index 0).
                make_call_ref_lines("target", recursive_call_byte, 0, Some(0)),
                // Bare call from sibling (enclosing = sibling, index 1).
                make_call_ref_lines("target", sibling_call_byte, 1, Some(1)),
            ],
            vec![
                make_symbol_with_kind_line_and_bytes(
                    "target",
                    SymbolKind::Function,
                    0,
                    (
                        target_byte,
                        target_byte + "fn target() { target(); }".len() as u32,
                    ),
                ),
                make_symbol_with_kind_line_and_bytes(
                    "sibling",
                    SymbolKind::Function,
                    1,
                    (
                        sibling_byte,
                        sibling_byte + "fn sibling() { target(); }".len() as u32,
                    ),
                ),
            ],
        );
        let index = make_index(vec![("src/recur.rs", target)], false);

        let view = index.capture_context_bundle_view("src/recur.rs", "target", Some("fn"), Some(1));

        let ContextBundleView::Found(found) = view else {
            panic!("expected found view");
        };

        // Both bare calls (recursion + sibling) remain exact callers; the receiver
        // guard suppresses neither because neither is preceded by a `.`.
        assert_eq!(
            found.callers.total_count, 2,
            "bare calls must still be counted as callers; got {:?}",
            found.callers.entries
        );
        assert!(
            found.unresolved_same_name_member_calls.is_empty(),
            "bare calls must not be surfaced as unresolved member calls"
        );
    }

    #[test]
    fn test_capture_context_bundle_view_exact_type_usages_exclude_unrelated_same_name_hits() {
        let target = make_file_with_refs_and_content(
            "src/db.rs",
            LanguageId::Rust,
            "pub struct Client;\npub struct Client;\n",
            vec![],
            vec![
                make_symbol_with_kind_line_and_bytes("Client", SymbolKind::Struct, 1, (0, 18)),
                make_symbol_with_kind_line_and_bytes("Client", SymbolKind::Struct, 2, (19, 37)),
            ],
        );
        let dependent = make_file_with_refs_and_content(
            "src/service.rs",
            LanguageId::Rust,
            "use crate::db::Client;\nstruct Holder { client: Client }\n",
            vec![
                make_ref("db", Some("crate::db"), ReferenceKind::Import, Some(0), 0),
                make_ref(
                    "Client",
                    Some("crate::db::Client"),
                    ReferenceKind::TypeUsage,
                    Some(0),
                    100,
                ),
            ],
            vec![make_symbol_with_kind_and_line(
                "Holder",
                SymbolKind::Struct,
                2,
            )],
        );
        let unrelated = make_file_with_refs_and_content(
            "src/other.rs",
            LanguageId::Rust,
            "struct Holder { client: Client }\n",
            vec![make_ref(
                "Client",
                None,
                ReferenceKind::TypeUsage,
                Some(0),
                0,
            )],
            vec![make_symbol_with_kind_and_line(
                "Holder",
                SymbolKind::Struct,
                1,
            )],
        );
        let index = make_index(
            vec![
                ("src/db.rs", target),
                ("src/service.rs", dependent),
                ("src/other.rs", unrelated),
            ],
            false,
        );

        let view =
            index.capture_context_bundle_view("src/db.rs", "Client", Some("struct"), Some(2));

        let ContextBundleView::Found(found) = view else {
            panic!("expected found view");
        };

        assert_eq!(found.type_usages.total_count, 1);
        assert_eq!(found.type_usages.entries.len(), 1);
        assert_eq!(found.type_usages.entries[0].file_path, "src/service.rs");
    }

    #[test]
    fn test_capture_context_bundle_view_collects_impl_block_suggestions_for_zero_caller_struct() {
        let content = "\
struct MyActor;

impl MyActor {
    fn new() -> Self { Self }
}

impl Actor for MyActor {
    fn handle(&self) {}
}
";
        let struct_start = content.find("struct MyActor;").unwrap() as u32;
        let inherent_impl = "impl MyActor {\n    fn new() -> Self { Self }\n}";
        let trait_impl = "impl Actor for MyActor {\n    fn handle(&self) {}\n}";
        let inherent_start = content.find(inherent_impl).unwrap() as u32;
        let trait_start = content.find(trait_impl).unwrap() as u32;
        let target = make_file_with_refs_and_content(
            "src/actors.rs",
            LanguageId::Rust,
            content,
            vec![],
            vec![
                make_symbol_with_kind_line_and_bytes(
                    "MyActor",
                    SymbolKind::Struct,
                    0,
                    (struct_start, struct_start + "struct MyActor;".len() as u32),
                ),
                make_symbol_with_kind_line_and_bytes(
                    "impl MyActor",
                    SymbolKind::Impl,
                    2,
                    (inherent_start, inherent_start + inherent_impl.len() as u32),
                ),
                make_symbol_with_kind_line_and_bytes(
                    "impl Actor for MyActor",
                    SymbolKind::Impl,
                    6,
                    (trait_start, trait_start + trait_impl.len() as u32),
                ),
            ],
        );
        let index = make_index(vec![("src/actors.rs", target)], false);

        let view =
            index.capture_context_bundle_view("src/actors.rs", "MyActor", Some("struct"), Some(1));

        let ContextBundleView::Found(found) = view else {
            panic!("expected found view");
        };

        assert_eq!(found.callers.total_count, 0);
        assert_eq!(found.implementation_suggestions.len(), 2);
        assert_eq!(
            found.implementation_suggestions[0].display_name,
            "impl MyActor"
        );
        assert_eq!(found.implementation_suggestions[0].line_number, 3);
        assert_eq!(
            found.implementation_suggestions[1].display_name,
            "impl Actor for MyActor"
        );
        assert_eq!(found.implementation_suggestions[1].line_number, 7);
    }

    #[test]
    fn test_find_files_by_basename_returns_sorted_paths() {
        let f1 = make_indexed_file("src/lib.rs", vec![], ParseStatus::Parsed);
        let f2 = make_indexed_file("tests/lib.rs", vec![], ParseStatus::Parsed);
        let f3 = make_indexed_file("src/main.rs", vec![], ParseStatus::Parsed);
        let index = make_index(
            vec![
                ("tests/lib.rs", f2),
                ("src/main.rs", f3),
                ("src/lib.rs", f1),
            ],
            false,
        );

        assert_eq!(
            index.find_files_by_basename("LIB.RS"),
            vec!["src/lib.rs", "tests/lib.rs"]
        );
    }

    #[test]
    fn test_find_files_by_dir_component_returns_sorted_paths() {
        let f1 = make_indexed_file("src/live_index/mod.rs", vec![], ParseStatus::Parsed);
        let f2 = make_indexed_file("src/live_index/store.rs", vec![], ParseStatus::Parsed);
        let f3 = make_indexed_file("tests/store.rs", vec![], ParseStatus::Parsed);
        let index = make_index(
            vec![
                ("src/live_index/store.rs", f2),
                ("tests/store.rs", f3),
                ("src/live_index/mod.rs", f1),
            ],
            false,
        );

        assert_eq!(
            index.find_files_by_dir_component("live_index"),
            vec!["src/live_index/mod.rs", "src/live_index/store.rs"]
        );
        assert_eq!(
            index.find_files_by_dir_component("SRC"),
            vec!["src/live_index/mod.rs", "src/live_index/store.rs"]
        );
    }

    #[test]
    fn test_file_count_correct() {
        let f1 = make_indexed_file("a.rs", vec![], ParseStatus::Parsed);
        let f2 = make_indexed_file("b.rs", vec![], ParseStatus::Parsed);
        let f3 = make_indexed_file("c.rs", vec![], ParseStatus::Parsed);
        let index = make_index(vec![("a.rs", f1), ("b.rs", f2), ("c.rs", f3)], false);
        assert_eq!(index.file_count(), 3);
    }

    #[test]
    fn test_symbol_count_across_all_files() {
        let f1 = make_indexed_file(
            "a.rs",
            vec![make_symbol("x"), make_symbol("y")],
            ParseStatus::Parsed,
        );
        let f2 = make_indexed_file("b.rs", vec![make_symbol("z")], ParseStatus::Parsed);
        let index = make_index(vec![("a.rs", f1), ("b.rs", f2)], false);
        assert_eq!(index.symbol_count(), 3);
    }

    #[test]
    fn test_health_stats_correct_breakdown() {
        let f1 = make_indexed_file("a.rs", vec![make_symbol("x")], ParseStatus::Parsed);
        let f2 = make_indexed_file(
            "b.rs",
            vec![make_symbol("y")],
            ParseStatus::PartialParse {
                warning: "syntax err".to_string(),
            },
        );
        let f3 = make_indexed_file(
            "c.rs",
            vec![],
            ParseStatus::Failed {
                error: "failed".to_string(),
            },
        );
        let index = make_index(vec![("a.rs", f1), ("b.rs", f2), ("c.rs", f3)], false);

        let stats = index.health_stats();
        assert_eq!(stats.file_count, 3);
        assert_eq!(stats.symbol_count, 2);
        assert_eq!(stats.parsed_count, 1);
        assert_eq!(stats.partial_parse_count, 1);
        assert_eq!(stats.failed_count, 1);
        assert_eq!(stats.partial_parse_files, vec!["b.rs".to_string()]);
    }

    #[test]
    fn test_health_stats_categorizes_expected_vendor_scss_parser_partials() {
        let vendor_c = make_indexed_file_with_language(
            "vendor/tree-sitter-scss/src/parser.c",
            LanguageId::C,
            vec![],
            ParseStatus::PartialParse {
                warning: "tree-sitter reported syntax errors".to_string(),
            },
        );
        let vendor_h = make_indexed_file_with_language(
            "vendor/tree-sitter-scss/src/tree_sitter/parser.h",
            LanguageId::C,
            vec![],
            ParseStatus::PartialParse {
                warning: "tree-sitter reported syntax errors".to_string(),
            },
        );
        let repo_rust = make_indexed_file(
            "src/broken.rs",
            vec![],
            ParseStatus::PartialParse {
                warning: "tree-sitter reported syntax errors".to_string(),
            },
        );
        let index = make_index(
            vec![
                ("vendor/tree-sitter-scss/src/parser.c", vendor_c),
                ("vendor/tree-sitter-scss/src/tree_sitter/parser.h", vendor_h),
                ("src/broken.rs", repo_rust),
            ],
            false,
        );

        let stats = index.health_stats();

        assert_eq!(stats.partial_parse_count, 3);
        assert_eq!(stats.unexpected_partial_parse_count, 1);
        assert_eq!(stats.expected_vendor_partial_parse_count, 2);
        assert_eq!(
            stats.unexpected_partial_parse_files,
            vec!["src/broken.rs".to_string()]
        );
        assert_eq!(
            stats.expected_vendor_partial_parse_files,
            vec![
                "vendor/tree-sitter-scss/src/parser.c".to_string(),
                "vendor/tree-sitter-scss/src/tree_sitter/parser.h".to_string(),
            ]
        );
    }

    #[test]
    fn test_health_stats_does_not_mark_all_vendor_partials_expected() {
        let vendor_c = make_indexed_file_with_language(
            "vendor/other-parser/src/parser.c",
            LanguageId::C,
            vec![],
            ParseStatus::PartialParse {
                warning: "tree-sitter reported syntax errors".to_string(),
            },
        );
        let index = make_index(vec![("vendor/other-parser/src/parser.c", vendor_c)], false);

        let stats = index.health_stats();

        assert_eq!(stats.partial_parse_count, 1);
        assert_eq!(stats.unexpected_partial_parse_count, 1);
        assert_eq!(stats.expected_vendor_partial_parse_count, 0);
        assert_eq!(
            stats.unexpected_partial_parse_files,
            vec!["vendor/other-parser/src/parser.c".to_string()]
        );
        assert!(stats.expected_vendor_partial_parse_files.is_empty());
    }

    #[test]
    fn test_is_ready_true_when_not_tripped() {
        let index = make_index(vec![], false);
        assert!(index.is_ready());
    }

    #[test]
    fn test_is_ready_false_when_tripped() {
        // Build a tripped circuit breaker by direct manipulation
        let cb = CircuitBreakerState::new(0.20);
        for _ in 0..7 {
            cb.record_success();
        }
        for i in 0..3 {
            cb.record_failure(&format!("f{i}.rs"), "err");
        }
        cb.should_abort(); // This will trip it

        let index = LiveIndex {
            files: HashMap::new(),
            loaded_at: Instant::now(),
            loaded_at_system: std::time::SystemTime::now(),
            load_duration: Duration::from_millis(10),
            cb_state: cb,
            is_empty: false,
            load_source: crate::live_index::store::IndexLoadSource::FreshLoad,
            snapshot_verify_state: crate::live_index::store::SnapshotVerifyState::NotNeeded,
            reverse_index: std::collections::HashMap::new(),
            files_by_basename: std::collections::HashMap::new(),
            files_by_dir_component: std::collections::HashMap::new(),
            trigram_index: crate::live_index::trigram::TrigramIndex::new(),
            gitignore: None,
            skipped_files: Vec::new(),
            coupling_store: None,
            local_empty_reason: Arc::new(parking_lot::RwLock::new(None)),
            indexed_root: None,
        };
        assert!(!index.is_ready());
    }

    #[test]
    fn test_index_state_ready() {
        let index = make_index(vec![], false);
        assert_eq!(index.index_state(), IndexState::Ready);
    }

    #[test]
    fn test_index_state_circuit_breaker_tripped_with_summary() {
        let cb = CircuitBreakerState::new(0.20);
        for _ in 0..7 {
            cb.record_success();
        }
        for i in 0..3 {
            cb.record_failure(&format!("f{i}.rs"), "err");
        }
        cb.should_abort();

        let index = LiveIndex {
            files: HashMap::new(),
            loaded_at: Instant::now(),
            loaded_at_system: std::time::SystemTime::now(),
            load_duration: Duration::from_millis(10),
            cb_state: cb,
            is_empty: false,
            load_source: crate::live_index::store::IndexLoadSource::FreshLoad,
            snapshot_verify_state: crate::live_index::store::SnapshotVerifyState::NotNeeded,
            reverse_index: std::collections::HashMap::new(),
            files_by_basename: std::collections::HashMap::new(),
            files_by_dir_component: std::collections::HashMap::new(),
            trigram_index: crate::live_index::trigram::TrigramIndex::new(),
            gitignore: None,
            skipped_files: Vec::new(),
            coupling_store: None,
            local_empty_reason: Arc::new(parking_lot::RwLock::new(None)),
            indexed_root: None,
        };

        match index.index_state() {
            IndexState::CircuitBreakerTripped { summary } => {
                assert!(!summary.is_empty(), "summary should not be empty");
            }
            other => panic!("expected CircuitBreakerTripped, got {:?}", other),
        }
    }

    // --- Extended HealthStats with watcher fields ---

    #[test]
    fn test_health_stats_default_watcher_fields() {
        let index = make_index(vec![], false);
        let stats = index.health_stats();
        assert_eq!(
            stats.watcher_state,
            WatcherState::Off,
            "default watcher state should be Off"
        );
        assert_eq!(
            stats.events_processed, 0,
            "default events_processed should be 0"
        );
        assert!(
            stats.last_event_at.is_none(),
            "default last_event_at should be None"
        );
        assert_eq!(
            stats.debounce_window_ms, 200,
            "default debounce_window_ms should be 200"
        );
    }

    #[test]
    fn test_health_stats_with_watcher_active() {
        let index = make_index(vec![], false);
        let now = SystemTime::now();
        let watcher = WatcherInfo {
            state: WatcherState::Active,
            events_processed: 42,
            last_event_at: Some(now),
            debounce_window_ms: 500,
            overflow_count: 3,
            last_overflow_at: Some(now),
            stale_files_found: 9,
            last_reconcile_at: Some(now),
        };
        let stats = index.health_stats_with_watcher(&watcher);
        assert_eq!(stats.watcher_state, WatcherState::Active);
        assert_eq!(stats.events_processed, 42);
        assert_eq!(stats.last_event_at, Some(now));
        assert_eq!(stats.debounce_window_ms, 500);
        assert_eq!(stats.overflow_count, 3);
        assert_eq!(stats.last_overflow_at, Some(now));
        assert_eq!(stats.stale_files_found, 9);
        assert_eq!(stats.last_reconcile_at, Some(now));
    }

    // -----------------------------------------------------------------------
    // Cross-reference query tests (Task 1, Plan 04-02)
    // -----------------------------------------------------------------------

    // --- find_references_for_name: basic ---

    #[test]
    fn test_find_references_for_name_returns_all_matching() {
        // "foo" referenced in two files — both should be returned.
        let refs_a = vec![make_ref("foo", None, ReferenceKind::Call, None, 0)];
        let refs_b = vec![make_ref("foo", None, ReferenceKind::Call, None, 0)];
        let f_a = make_file_with_refs("a.rs", refs_a, HashMap::new());
        let f_b = make_file_with_refs("b.rs", refs_b, HashMap::new());
        let index = make_index(vec![("a.rs", f_a), ("b.rs", f_b)], false);

        let results = index.find_references_for_name("foo", None, false);
        assert_eq!(results.len(), 2, "both files should match");
    }

    #[test]
    fn test_find_references_for_name_kind_filter_call_only() {
        // Two references to "foo" in same file: one Call, one Import. Kind filter returns only Call.
        let refs = vec![
            make_ref("foo", None, ReferenceKind::Call, None, 0),
            make_ref("foo", None, ReferenceKind::Import, None, 100),
        ];
        let f = make_file_with_refs("a.rs", refs, HashMap::new());
        let index = make_index(vec![("a.rs", f)], false);

        let results = index.find_references_for_name("foo", Some(ReferenceKind::Call), false);
        assert_eq!(results.len(), 1, "only Call reference should be returned");
        assert_eq!(results[0].1.kind, ReferenceKind::Call);
    }

    #[test]
    fn test_find_references_for_name_kind_filter_excludes_import() {
        let refs = vec![make_ref("foo", None, ReferenceKind::Import, None, 0)];
        let f = make_file_with_refs("a.rs", refs, HashMap::new());
        let index = make_index(vec![("a.rs", f)], false);

        let results = index.find_references_for_name("foo", Some(ReferenceKind::Call), false);
        assert!(
            results.is_empty(),
            "Import reference should be excluded when filtering for Call"
        );
    }

    // --- Built-in filter (XREF-04 / XREF-06) ---

    #[test]
    fn test_find_references_builtin_string_filtered() {
        // "string" is a JS/TS built-in — should be filtered.
        let refs = vec![make_ref("string", None, ReferenceKind::TypeUsage, None, 0)];
        let f = make_file_with_refs_and_content(
            "a.ts",
            LanguageId::TypeScript,
            "type Alias = string;",
            refs,
            vec![],
        );
        let index = make_index(vec![("a.ts", f)], false);

        let results = index.find_references_for_name("string", None, false);
        assert!(
            results.is_empty(),
            "built-in 'string' should be filtered by default"
        );
    }

    #[test]
    fn test_find_references_builtin_i32_filtered() {
        let refs = vec![make_ref("i32", None, ReferenceKind::TypeUsage, None, 0)];
        let f = make_file_with_refs("a.rs", refs, HashMap::new());
        let index = make_index(vec![("a.rs", f)], false);

        let results = index.find_references_for_name("i32", None, false);
        assert!(results.is_empty(), "Rust built-in 'i32' should be filtered");
    }

    #[test]
    fn test_find_references_cross_language_builtin_does_not_filter_rust_symbol() {
        let rust_file = make_file_with_refs_and_content(
            "src/model.rs",
            LanguageId::Rust,
            "struct Object;",
            vec![make_ref("Object", None, ReferenceKind::TypeUsage, None, 0)],
            vec![],
        );
        let python_file = make_file_with_refs_and_content(
            "src/model.py",
            LanguageId::Python,
            "value: object",
            vec![make_ref("object", None, ReferenceKind::TypeUsage, None, 0)],
            vec![],
        );
        let index = make_index(
            vec![("src/model.rs", rust_file), ("src/model.py", python_file)],
            false,
        );

        let results = index.find_references_for_name("Object", None, false);
        assert_eq!(
            results.len(),
            1,
            "built-ins from other languages must not hide valid Rust symbols"
        );
        assert_eq!(results[0].0, "src/model.rs");
    }

    #[test]
    fn test_find_references_mystruct_not_filtered() {
        let refs = vec![make_ref(
            "MyStruct",
            None,
            ReferenceKind::TypeUsage,
            None,
            0,
        )];
        let f = make_file_with_refs("a.rs", refs, HashMap::new());
        let index = make_index(vec![("a.rs", f)], false);

        let results = index.find_references_for_name("MyStruct", None, false);
        assert_eq!(
            results.len(),
            1,
            "user-defined type 'MyStruct' should NOT be filtered"
        );
    }

    #[test]
    fn test_find_references_builtin_include_filtered_bypasses() {
        // include_filtered=true should return even built-in matches.
        let refs = vec![make_ref("i32", None, ReferenceKind::TypeUsage, None, 0)];
        let f = make_file_with_refs("a.rs", refs, HashMap::new());
        let index = make_index(vec![("a.rs", f)], false);

        let results = index.find_references_for_name("i32", None, true);
        assert_eq!(
            results.len(),
            1,
            "include_filtered=true should bypass the filter"
        );
    }

    // --- Generic filter ---

    #[test]
    fn test_find_references_single_letter_t_filtered() {
        let refs = vec![make_ref("T", None, ReferenceKind::TypeUsage, None, 0)];
        let f = make_file_with_refs("a.rs", refs, HashMap::new());
        let index = make_index(vec![("a.rs", f)], false);

        let results = index.find_references_for_name("T", None, false);
        assert!(
            results.is_empty(),
            "single-letter generic 'T' should be filtered"
        );
    }

    #[test]
    fn test_find_references_single_letter_k_filtered() {
        let refs = vec![make_ref("K", None, ReferenceKind::TypeUsage, None, 0)];
        let f = make_file_with_refs("a.rs", refs, HashMap::new());
        let index = make_index(vec![("a.rs", f)], false);

        let results = index.find_references_for_name("K", None, false);
        assert!(
            results.is_empty(),
            "single-letter generic 'K' should be filtered"
        );
    }

    #[test]
    fn test_find_references_multi_letter_key_not_filtered() {
        let refs = vec![make_ref("Key", None, ReferenceKind::TypeUsage, None, 0)];
        let f = make_file_with_refs("a.rs", refs, HashMap::new());
        let index = make_index(vec![("a.rs", f)], false);

        let results = index.find_references_for_name("Key", None, false);
        assert_eq!(
            results.len(),
            1,
            "multi-letter name 'Key' should NOT be filtered"
        );
    }

    // --- Alias resolution (XREF-05) ---

    #[test]
    fn test_find_references_alias_resolution_hashmap_via_map() {
        // File b.rs has a reference to "Map" with alias_map["Map"] = "HashMap".
        // Searching for "HashMap" should also return the "Map" reference.
        let mut alias_map = HashMap::new();
        alias_map.insert("Map".to_string(), "HashMap".to_string());

        let refs_b = vec![make_ref("Map", None, ReferenceKind::Call, None, 0)];
        let f_a = make_file_with_refs("a.rs", vec![], HashMap::new()); // no refs
        let f_b = make_file_with_refs("b.rs", refs_b, alias_map);
        let index = make_index(vec![("a.rs", f_a), ("b.rs", f_b)], false);

        let results = index.find_references_for_name("HashMap", None, false);
        // Should find the "Map" reference from b.rs via alias resolution
        assert!(
            !results.is_empty(),
            "alias resolution should find 'Map' when searching 'HashMap'"
        );
        assert_eq!(results[0].1.name, "Map");
    }

    // --- Qualified name matching ---

    #[test]
    fn test_find_references_qualified_name_vec_new() {
        let refs = vec![make_ref(
            "new",
            Some("Vec::new"),
            ReferenceKind::Call,
            None,
            0,
        )];
        let f = make_file_with_refs("a.rs", refs, HashMap::new());
        let index = make_index(vec![("a.rs", f)], false);

        // Qualified search: "Vec::new" matches against qualified_name field.
        let results = index.find_references_for_name("Vec::new", None, false);
        assert_eq!(
            results.len(),
            1,
            "qualified 'Vec::new' should match via qualified_name field"
        );
    }

    #[test]
    fn test_find_references_qualified_does_not_match_unqualified() {
        // "new" (simple) should not match when searching for qualified "Vec::new".
        let refs = vec![make_ref("new", None, ReferenceKind::Call, None, 0)]; // no qualified_name
        let f = make_file_with_refs("a.rs", refs, HashMap::new());
        let index = make_index(vec![("a.rs", f)], false);

        let results = index.find_references_for_name("Vec::new", None, false);
        assert!(
            results.is_empty(),
            "qualified search should not match reference without qualified_name"
        );
    }

    // --- Result fields ---

    #[test]
    fn test_find_references_result_includes_correct_file_path_and_record() {
        let refs = vec![make_ref("load", None, ReferenceKind::Call, None, 0)];
        let f = make_file_with_refs("src/loader.rs", refs, HashMap::new());
        let index = make_index(vec![("src/loader.rs", f)], false);

        let results = index.find_references_for_name("load", None, false);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "src/loader.rs", "file_path should match");
        assert_eq!(results[0].1.name, "load");
    }

    #[test]
    fn test_capture_find_references_view_for_symbol_scopes_to_dependent_files() {
        let target = make_file_with_refs_and_content(
            "src/db.rs",
            LanguageId::Rust,
            "pub fn connect() {}\n",
            vec![],
            vec![make_symbol_with_kind_and_line(
                "connect",
                SymbolKind::Function,
                1,
            )],
        );
        let dependent = make_file_with_refs_and_content(
            "src/service.rs",
            LanguageId::Rust,
            "use crate::db::connect;\nfn run() { connect(); }\n",
            vec![
                make_ref("db", Some("crate::db"), ReferenceKind::Import, None, 0),
                make_ref(
                    "connect",
                    Some("crate::db::connect"),
                    ReferenceKind::Call,
                    None,
                    100,
                ),
            ],
            vec![make_symbol("run")],
        );
        let unrelated = make_file_with_refs_and_content(
            "src/other.rs",
            LanguageId::Rust,
            "fn run() { connect(); }\n",
            vec![make_ref("connect", None, ReferenceKind::Call, None, 0)],
            vec![make_symbol("run")],
        );
        let index = make_index(
            vec![
                ("src/db.rs", target),
                ("src/service.rs", dependent),
                ("src/other.rs", unrelated),
            ],
            false,
        );

        let view = index
            .capture_find_references_view_for_symbol(
                "src/db.rs",
                "connect",
                Some("fn"),
                Some(2),
                Some("call"),
                200,
            )
            .expect("exact selector should resolve");

        assert_eq!(view.total_refs, 1);
        assert_eq!(view.files.len(), 1);
        assert_eq!(view.files[0].file_path, "src/service.rs");
    }

    #[test]
    fn test_capture_find_references_view_for_symbol_requires_line_for_ambiguous_selector() {
        let target = make_file_with_refs_and_content(
            "src/db.rs",
            LanguageId::Rust,
            "fn connect() {}\nfn connect() {}\n",
            vec![],
            vec![
                make_symbol_with_kind_and_line("connect", SymbolKind::Function, 1),
                make_symbol_with_kind_and_line("connect", SymbolKind::Function, 10),
            ],
        );
        let index = make_index(vec![("src/db.rs", target)], false);

        let error = index
            .capture_find_references_view_for_symbol(
                "src/db.rs",
                "connect",
                Some("fn"),
                None,
                Some("call"),
                200,
            )
            .expect_err("selector without line should be ambiguous");

        assert!(error.contains("Ambiguous symbol selector"), "got: {error}");
        assert!(error.contains("1"), "got: {error}");
        assert!(error.contains("10"), "got: {error}");
    }

    // --- find_dependents_for_file ---

    #[test]
    fn test_find_dependents_for_file_returns_importer() {
        // b.rs imports "db" — should be a dependent of src/db.rs.
        let import_ref = make_ref("db", None, ReferenceKind::Import, None, 0);
        let f_b = make_file_with_refs("src/b.rs", vec![import_ref], HashMap::new());
        let f_db = make_file_with_refs("src/db.rs", vec![], HashMap::new());
        let index = make_index(vec![("src/b.rs", f_b), ("src/db.rs", f_db)], false);

        let deps = index.find_dependents_for_file("src/db.rs");
        assert_eq!(
            deps.len(),
            1,
            "b.rs imports 'db' so it is a dependent of db.rs"
        );
        assert_eq!(deps[0].0, "src/b.rs");
    }

    #[test]
    fn test_find_dependents_no_importers_returns_empty() {
        let f = make_file_with_refs("src/db.rs", vec![], HashMap::new());
        let index = make_index(vec![("src/db.rs", f)], false);

        let deps = index.find_dependents_for_file("src/db.rs");
        assert!(deps.is_empty(), "no importers means empty dependents list");
    }

    #[test]
    fn test_find_dependents_excludes_self() {
        // A file that imports its own stem should not appear as its own dependent.
        let self_import = make_ref("db", None, ReferenceKind::Import, None, 0);
        let f_db = make_file_with_refs("src/db.rs", vec![self_import], HashMap::new());
        let index = make_index(vec![("src/db.rs", f_db)], false);

        let deps = index.find_dependents_for_file("src/db.rs");
        assert!(deps.is_empty(), "a file should not be its own dependent");
    }

    #[test]
    fn test_find_dependents_qualified_import_crate_db() {
        // b.rs has import "crate::db" — should match src/db.rs.
        let import_ref = make_ref("crate::db", None, ReferenceKind::Import, None, 0);
        let f_b = make_file_with_refs("src/b.rs", vec![import_ref], HashMap::new());
        let f_db = make_file_with_refs("src/db.rs", vec![], HashMap::new());
        let index = make_index(vec![("src/b.rs", f_b), ("src/db.rs", f_db)], false);

        let deps = index.find_dependents_for_file("src/db.rs");
        assert_eq!(
            deps.len(),
            1,
            "qualified 'crate::db' should match src/db.rs"
        );
    }

    #[test]
    fn test_find_dependents_workspace_crate_qualified_import() {
        // Workspace layout: crates/core/src/types.rs defines types,
        // crates/api/src/handler.rs imports "crate::types".
        // The module path for "crates/core/src/types.rs" should resolve to
        // "crate::types", matching the import in handler.rs.
        let import_ref = make_ref("crate::types", None, ReferenceKind::Import, None, 0);
        let type_usage = make_ref("MyType", None, ReferenceKind::TypeUsage, None, 5);
        let f_handler = make_file_with_refs(
            "crates/api/src/handler.rs",
            vec![import_ref, type_usage],
            HashMap::new(),
        );
        let my_type_sym = SymbolRecord {
            name: "MyType".to_string(),
            kind: SymbolKind::Struct,
            depth: 0,
            sort_order: 0,
            byte_range: (0, 30),
            line_range: (0, 1),
            doc_byte_range: None,
            item_byte_range: None,
        };
        let mut f_types = make_file_with_refs("crates/core/src/types.rs", vec![], HashMap::new());
        f_types.symbols.push(my_type_sym);
        let index = make_index(
            vec![
                ("crates/api/src/handler.rs", f_handler),
                ("crates/core/src/types.rs", f_types),
            ],
            false,
        );

        let deps = index.find_dependents_for_file("crates/core/src/types.rs");
        assert!(
            !deps.is_empty(),
            "workspace crate types.rs should have dependents, got: {:?}",
            deps.iter().map(|(p, _)| p).collect::<Vec<_>>()
        );
    }

    // --- find_dependents: module path resolution ---

    #[test]
    fn test_find_dependents_lib_rs_via_module_path() {
        // main.rs imports "crate::error" — lib.rs is the crate root, so it's a dependent.
        let import_ref = make_ref("crate::error", None, ReferenceKind::Import, None, 0);
        let f_main = make_file_with_refs("src/main.rs", vec![import_ref], HashMap::new());
        let f_lib = make_file_with_refs("src/lib.rs", vec![], HashMap::new());
        let index = make_index(vec![("src/main.rs", f_main), ("src/lib.rs", f_lib)], false);

        let deps = index.find_dependents_for_file("src/lib.rs");
        assert_eq!(
            deps.len(),
            1,
            "crate::error starts with 'crate' module path of lib.rs"
        );
        assert_eq!(deps[0].0, "src/main.rs");
    }

    #[test]
    fn test_find_dependents_mod_rs_via_module_path() {
        // store.rs imports "crate::live_index" — mod.rs defines that module.
        let import_ref = make_ref(
            "crate::live_index::store",
            None,
            ReferenceKind::Import,
            None,
            0,
        );
        let f_store = make_file_with_refs("src/store.rs", vec![import_ref], HashMap::new());
        let f_mod = make_file_with_refs("src/live_index/mod.rs", vec![], HashMap::new());
        let index = make_index(
            vec![("src/store.rs", f_store), ("src/live_index/mod.rs", f_mod)],
            false,
        );

        let deps = index.find_dependents_for_file("src/live_index/mod.rs");
        assert_eq!(
            deps.len(),
            1,
            "'crate::live_index::store' starts with module path 'crate::live_index'"
        );
        assert_eq!(deps[0].0, "src/store.rs");
    }

    #[test]
    fn test_find_dependents_rust_returns_symbol_usage_when_module_import_matches() {
        let target = make_file_with_refs_and_content(
            "src/daemon.rs",
            LanguageId::Rust,
            "pub fn connect_or_spawn_session() {}",
            vec![],
            vec![make_symbol("connect_or_spawn_session")],
        );
        let dependent = make_file_with_refs_and_content(
            "src/main.rs",
            LanguageId::Rust,
            "use crate::{daemon, other}; fn main() { daemon::connect_or_spawn_session(); }",
            vec![
                make_ref(
                    "daemon",
                    Some("crate::daemon"),
                    ReferenceKind::Import,
                    None,
                    0,
                ),
                make_ref(
                    "connect_or_spawn_session",
                    Some("daemon::connect_or_spawn_session"),
                    ReferenceKind::Call,
                    Some(0),
                    100,
                ),
            ],
            vec![make_symbol("main")],
        );
        let index = make_index(
            vec![("src/daemon.rs", target), ("src/main.rs", dependent)],
            false,
        );

        let deps = index.find_dependents_for_file("src/daemon.rs");
        assert_eq!(
            deps.len(),
            1,
            "matched Rust module imports should surface actual symbol usage, not just import stubs"
        );
        assert_eq!(deps[0].0, "src/main.rs");
        assert_eq!(deps[0].1.kind, ReferenceKind::Call);
        assert_eq!(deps[0].1.name, "connect_or_spawn_session");
    }

    #[test]
    fn test_find_dependents_python_init_via_module_path() {
        // app.py imports "utils.helpers" — __init__.py defines the utils package.
        let import_ref = make_ref("utils.helpers", None, ReferenceKind::Import, None, 0);
        let mut f_app = make_file_with_refs("src/app.py", vec![import_ref], HashMap::new());
        f_app.language = LanguageId::Python;
        let mut f_init = make_file_with_refs("utils/__init__.py", vec![], HashMap::new());
        f_init.language = LanguageId::Python;
        let index = make_index(
            vec![("src/app.py", f_app), ("utils/__init__.py", f_init)],
            false,
        );

        let deps = index.find_dependents_for_file("utils/__init__.py");
        assert_eq!(
            deps.len(),
            1,
            "'utils.helpers' starts with module path 'utils'"
        );
    }

    #[test]
    fn test_find_dependents_rust_target_excludes_python_bare_module_import() {
        // Regression: a Python `import gguf` (referring to the Python `gguf`
        // package) must NOT be reported as a dependent of the unrelated Rust file
        // `launcher/src/gguf.rs`. Import resolution is language-scoped; matching the
        // two purely by the shared bare module name "gguf" is the bug under test.
        //
        // Meanwhile a genuine same-language Rust importer of `gguf` MUST still be
        // returned, so the language guard does not drop legitimate dependents.
        let py_import = make_ref("gguf", None, ReferenceKind::Import, None, 0);
        let mut f_py = make_file_with_refs(
            "llama-cpp/convert_hf_to_gguf.py",
            vec![py_import],
            HashMap::new(),
        );
        f_py.language = LanguageId::Python;

        let rust_import = make_ref("gguf", None, ReferenceKind::Import, None, 0);
        let f_rust = make_file_with_refs("launcher/src/main.rs", vec![rust_import], HashMap::new());

        let f_target = make_file_with_refs("launcher/src/gguf.rs", vec![], HashMap::new());

        let index = make_index(
            vec![
                ("llama-cpp/convert_hf_to_gguf.py", f_py),
                ("launcher/src/main.rs", f_rust),
                ("launcher/src/gguf.rs", f_target),
            ],
            false,
        );

        let deps = index.find_dependents_for_file("launcher/src/gguf.rs");
        let dep_paths: Vec<&str> = deps.iter().map(|(p, _)| *p).collect();

        assert!(
            !dep_paths.contains(&"llama-cpp/convert_hf_to_gguf.py"),
            "a Python `import gguf` must not be a dependent of Rust gguf.rs, got: {dep_paths:?}"
        );
        assert!(
            dep_paths.contains(&"launcher/src/main.rs"),
            "a same-language Rust importer of `gguf` must still be a dependent, got: {dep_paths:?}"
        );
        assert_eq!(
            dep_paths.len(),
            1,
            "only the same-language Rust importer should match, got: {dep_paths:?}"
        );
    }

    #[test]
    fn test_find_dependents_python_target_excludes_rust_bare_module_import() {
        // Reverse direction: a Rust `use gguf` must NOT be reported as a dependent
        // of an unrelated Python `gguf.py`, while a same-language Python importer is.
        let rust_import = make_ref("gguf", None, ReferenceKind::Import, None, 0);
        let f_rust = make_file_with_refs("launcher/src/main.rs", vec![rust_import], HashMap::new());

        let py_import = make_ref("gguf", None, ReferenceKind::Import, None, 0);
        let mut f_py = make_file_with_refs("app.py", vec![py_import], HashMap::new());
        f_py.language = LanguageId::Python;

        let mut f_target = make_file_with_refs("gguf-py/gguf/gguf.py", vec![], HashMap::new());
        f_target.language = LanguageId::Python;

        let index = make_index(
            vec![
                ("launcher/src/main.rs", f_rust),
                ("app.py", f_py),
                ("gguf-py/gguf/gguf.py", f_target),
            ],
            false,
        );

        let deps = index.find_dependents_for_file("gguf-py/gguf/gguf.py");
        let dep_paths: Vec<&str> = deps.iter().map(|(p, _)| *p).collect();

        assert!(
            !dep_paths.contains(&"launcher/src/main.rs"),
            "a Rust `use gguf` must not be a dependent of Python gguf.py, got: {dep_paths:?}"
        );
        assert!(
            dep_paths.contains(&"app.py"),
            "a same-language Python importer of `gguf` must still be a dependent, got: {dep_paths:?}"
        );
        assert_eq!(
            dep_paths.len(),
            1,
            "only the same-language Python importer should match, got: {dep_paths:?}"
        );
    }

    #[test]
    fn test_find_dependents_js_ts_interop_still_matches() {
        // Guard: the JavaScript/TypeScript family genuinely imports across the
        // boundary, so a `.ts` importer of a `.js` module must still be a dependent.
        let ts_import = make_ref("widget", None, ReferenceKind::Import, None, 0);
        let mut f_ts = make_file_with_refs("src/app.ts", vec![ts_import], HashMap::new());
        f_ts.language = LanguageId::TypeScript;

        let mut f_widget = make_file_with_refs("src/widget.js", vec![], HashMap::new());
        f_widget.language = LanguageId::JavaScript;

        let index = make_index(
            vec![("src/app.ts", f_ts), ("src/widget.js", f_widget)],
            false,
        );

        let deps = index.find_dependents_for_file("src/widget.js");
        let dep_paths: Vec<&str> = deps.iter().map(|(p, _)| *p).collect();
        assert!(
            dep_paths.contains(&"src/app.ts"),
            "a .ts importer of a .js module must remain a dependent (JS/TS interop), got: {dep_paths:?}"
        );
    }

    #[test]
    fn test_find_dependents_js_index_via_module_path() {
        // app.js imports "src/utils" — index.ts defines that directory module.
        let import_ref = make_ref("src/utils/foo", None, ReferenceKind::Import, None, 0);
        let mut f_app = make_file_with_refs("src/app.js", vec![import_ref], HashMap::new());
        f_app.language = LanguageId::JavaScript;
        let mut f_index = make_file_with_refs("src/utils/index.js", vec![], HashMap::new());
        f_index.language = LanguageId::JavaScript;
        let index = make_index(
            vec![("src/app.js", f_app), ("src/utils/index.js", f_index)],
            false,
        );

        let deps = index.find_dependents_for_file("src/utils/index.js");
        assert_eq!(
            deps.len(),
            1,
            "'src/utils/foo' starts with module path 'src/utils'"
        );
    }

    #[test]
    fn test_find_dependents_csharp_type_usage_with_imported_namespace() {
        let target = make_file_with_refs_and_content(
            "Core/Services/IMinioService.cs",
            LanguageId::CSharp,
            "namespace CeRegistry.Core.Services { public interface IMinioService {} }",
            vec![],
            vec![SymbolRecord {
                name: "IMinioService".to_string(),
                kind: SymbolKind::Interface,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 10),
                line_range: (0, 0),
                doc_byte_range: None,
                item_byte_range: None,
            }],
        );
        let dependent = make_file_with_refs_and_content(
            "Api/Controllers/PacketsController.cs",
            LanguageId::CSharp,
            r#"using CeRegistry.Core.Services;
namespace CeRegistry.Api.Controllers {
    public class PacketsController {
        private readonly IMinioService _minio;
        public PacketsController(IMinioService minioService) {}
    }
}"#,
            vec![
                make_ref(
                    "Services",
                    Some("CeRegistry.Core.Services"),
                    ReferenceKind::Import,
                    None,
                    0,
                ),
                make_ref(
                    "IMinioService",
                    None,
                    ReferenceKind::TypeUsage,
                    Some(0),
                    100,
                ),
                make_ref(
                    "IMinioService",
                    None,
                    ReferenceKind::TypeUsage,
                    Some(0),
                    200,
                ),
            ],
            vec![make_symbol("PacketsController")],
        );
        let index = make_index(
            vec![
                ("Core/Services/IMinioService.cs", target),
                ("Api/Controllers/PacketsController.cs", dependent),
            ],
            false,
        );

        let deps = index.find_dependents_for_file("Core/Services/IMinioService.cs");
        assert_eq!(
            deps.len(),
            2,
            "constructor and field type usage should both be treated as dependencies"
        );
        assert!(
            deps.iter()
                .all(|(path, _)| *path == "Api/Controllers/PacketsController.cs")
        );
        assert!(deps.iter().all(|(_, r)| r.kind == ReferenceKind::TypeUsage));
    }

    #[test]
    fn test_find_dependents_csharp_type_usage_in_same_namespace_without_import() {
        let target = make_file_with_refs_and_content(
            "Core/Services/IMinioService.cs",
            LanguageId::CSharp,
            "namespace CeRegistry.Core.Services { public interface IMinioService {} }",
            vec![],
            vec![SymbolRecord {
                name: "IMinioService".to_string(),
                kind: SymbolKind::Interface,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 10),
                line_range: (0, 0),
                doc_byte_range: None,
                item_byte_range: None,
            }],
        );
        let dependent = make_file_with_refs_and_content(
            "Core/Services/MinioServiceConsumer.cs",
            LanguageId::CSharp,
            r#"namespace CeRegistry.Core.Services {
    public class MinioServiceConsumer {
        private readonly IMinioService _minio;
    }
}"#,
            vec![make_ref(
                "IMinioService",
                None,
                ReferenceKind::TypeUsage,
                Some(0),
                100,
            )],
            vec![make_symbol("MinioServiceConsumer")],
        );
        let index = make_index(
            vec![
                ("Core/Services/IMinioService.cs", target),
                ("Core/Services/MinioServiceConsumer.cs", dependent),
            ],
            false,
        );

        let deps = index.find_dependents_for_file("Core/Services/IMinioService.cs");
        assert_eq!(
            deps.len(),
            1,
            "same-namespace C# type usage should count even without a using directive"
        );
        assert_eq!(deps[0].0, "Core/Services/MinioServiceConsumer.cs");
        assert_eq!(deps[0].1.kind, ReferenceKind::TypeUsage);
    }

    #[test]
    fn test_find_dependents_java_type_usage_with_imported_package() {
        let target = make_file_with_refs_and_content(
            "src/main/java/com/acme/storage/MinioService.java",
            LanguageId::Java,
            "package com.acme.storage; public class MinioService {}",
            vec![],
            vec![SymbolRecord {
                name: "MinioService".to_string(),
                kind: SymbolKind::Class,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 10),
                line_range: (0, 0),
                doc_byte_range: None,
                item_byte_range: None,
            }],
        );
        let dependent = make_file_with_refs_and_content(
            "src/main/java/com/acme/api/PacketsController.java",
            LanguageId::Java,
            r#"package com.acme.api;
import com.acme.storage.MinioService;
public class PacketsController {
    private final MinioService minioService;
}"#,
            vec![
                make_ref(
                    "MinioService",
                    Some("com.acme.storage.MinioService"),
                    ReferenceKind::Import,
                    None,
                    0,
                ),
                make_ref("MinioService", None, ReferenceKind::TypeUsage, Some(0), 100),
            ],
            vec![make_symbol("PacketsController")],
        );
        let index = make_index(
            vec![
                ("src/main/java/com/acme/storage/MinioService.java", target),
                (
                    "src/main/java/com/acme/api/PacketsController.java",
                    dependent,
                ),
            ],
            false,
        );

        let deps =
            index.find_dependents_for_file("src/main/java/com/acme/storage/MinioService.java");
        assert_eq!(
            deps.len(),
            1,
            "Java field type usage should resolve through the imported package"
        );
        assert_eq!(
            deps[0].0,
            "src/main/java/com/acme/api/PacketsController.java"
        );
        assert_eq!(deps[0].1.kind, ReferenceKind::TypeUsage);
    }

    #[test]
    fn test_find_dependents_typescript_prefers_type_usage_when_module_import_matches() {
        let target = make_file_with_refs_and_content(
            "src/service.ts",
            LanguageId::TypeScript,
            "export class Service {}",
            vec![],
            vec![SymbolRecord {
                name: "Service".to_string(),
                kind: SymbolKind::Class,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 10),
                line_range: (0, 0),
                doc_byte_range: None,
                item_byte_range: None,
            }],
        );
        let dependent = make_file_with_refs_and_content(
            "src/app.ts",
            LanguageId::TypeScript,
            "import { Service } from \"./service\";\nexport class App { constructor(private service: Service) {} }",
            vec![
                make_ref("service", Some("./service"), ReferenceKind::Import, None, 0),
                make_ref("Service", None, ReferenceKind::TypeUsage, Some(0), 100),
            ],
            vec![SymbolRecord {
                name: "App".to_string(),
                kind: SymbolKind::Class,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 10),
                line_range: (0, 0),
                doc_byte_range: None,
                item_byte_range: None,
            }],
        );
        let index = make_index(
            vec![("src/service.ts", target), ("src/app.ts", dependent)],
            false,
        );

        let deps = index.find_dependents_for_file("src/service.ts");
        assert_eq!(
            deps.len(),
            1,
            "module-backed TypeScript dependents should report type usage, not just the import"
        );
        assert_eq!(deps[0].0, "src/app.ts");
        assert_eq!(deps[0].1.kind, ReferenceKind::TypeUsage);
        assert_eq!(deps[0].1.name, "Service");
    }

    #[test]
    fn test_find_dependents_follows_pub_use_reexport_chain() {
        // src/domain/index.rs defines ReferenceKind
        // src/domain/mod.rs has `pub use index::ReferenceKind;`
        // src/tools.rs has `use crate::domain::ReferenceKind;`
        //
        // find_dependents("src/domain/index.rs") should find src/tools.rs
        // via the re-export chain: index.rs -> mod.rs -> tools.rs

        let target = make_file_with_refs_and_content(
            "src/domain/index.rs",
            LanguageId::Rust,
            "pub enum ReferenceKind { Call, Import }",
            vec![],
            vec![SymbolRecord {
                name: "ReferenceKind".to_string(),
                kind: SymbolKind::Enum,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 38),
                line_range: (0, 0),
                doc_byte_range: None,
                item_byte_range: None,
            }],
        );

        // mod.rs content: "pub use index::ReferenceKind;\n"
        // The import ref starts at byte 8 ("index::ReferenceKind")
        let mod_content = "pub use index::ReferenceKind;\n";
        let reexporter = make_file_with_refs_and_content(
            "src/domain/mod.rs",
            LanguageId::Rust,
            mod_content,
            vec![make_ref(
                "ReferenceKind",
                Some("index::ReferenceKind"),
                ReferenceKind::Import,
                None,
                8, // byte offset where "index::ReferenceKind" starts
            )],
            vec![],
        );

        // tools.rs imports via crate::domain::ReferenceKind
        let consumer = make_file_with_refs_and_content(
            "src/tools.rs",
            LanguageId::Rust,
            "use crate::domain::ReferenceKind;\nfn check(r: ReferenceKind) {}",
            vec![
                make_ref(
                    "ReferenceKind",
                    Some("crate::domain::ReferenceKind"),
                    ReferenceKind::Import,
                    None,
                    0,
                ),
                make_ref(
                    "ReferenceKind",
                    None,
                    ReferenceKind::TypeUsage,
                    Some(0),
                    100,
                ),
            ],
            vec![make_symbol("check")],
        );

        let index = make_index(
            vec![
                ("src/domain/index.rs", target),
                ("src/domain/mod.rs", reexporter),
                ("src/tools.rs", consumer),
            ],
            false,
        );

        let deps = index.find_dependents_for_file("src/domain/index.rs");
        // mod.rs is a direct dependent (it imports from index.rs)
        // tools.rs is a transitive dependent (it imports from mod.rs which re-exports from index.rs)
        let dep_files: Vec<&str> = deps.iter().map(|(p, _)| *p).collect();
        assert!(
            dep_files.contains(&"src/domain/mod.rs"),
            "mod.rs should be a direct dependent, got: {dep_files:?}"
        );
        assert!(
            dep_files.contains(&"src/tools.rs"),
            "tools.rs should be found via pub use re-export chain, got: {dep_files:?}"
        );
    }

    #[test]
    fn test_find_dependents_lib_rs_reexport_finds_transitive() {
        // src/lib.rs has `pub use error::AppError;`
        // src/error.rs defines AppError
        // src/main.rs has `use crate::AppError;`
        //
        // find_dependents("src/error.rs") should find main.rs via the
        // lib.rs re-export chain.

        let target = make_file_with_refs_and_content(
            "src/error.rs",
            LanguageId::Rust,
            "pub struct AppError { msg: String }",
            vec![],
            vec![SymbolRecord {
                name: "AppError".to_string(),
                kind: SymbolKind::Struct,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 35),
                line_range: (0, 0),
                doc_byte_range: None,
                item_byte_range: None,
            }],
        );

        // lib.rs re-exports from error module
        let lib_content = "pub mod error;\npub use error::AppError;\n";
        let lib_file = make_file_with_refs_and_content(
            "src/lib.rs",
            LanguageId::Rust,
            lib_content,
            vec![make_ref(
                "AppError",
                Some("error::AppError"),
                ReferenceKind::Import,
                None,
                24, // byte offset of "error::AppError" in the content
            )],
            vec![],
        );

        // main.rs imports AppError from crate root (lib.rs)
        let consumer = make_file_with_refs_and_content(
            "src/main.rs",
            LanguageId::Rust,
            "use crate::AppError;\nfn main() { let _e = AppError { msg: String::new() }; }",
            vec![
                make_ref(
                    "AppError",
                    Some("crate::AppError"),
                    ReferenceKind::Import,
                    None,
                    0,
                ),
                make_ref("AppError", None, ReferenceKind::TypeUsage, Some(0), 100),
            ],
            vec![make_symbol("main")],
        );

        let index = make_index(
            vec![
                ("src/error.rs", target),
                ("src/lib.rs", lib_file),
                ("src/main.rs", consumer),
            ],
            false,
        );

        let deps = index.find_dependents_for_file("src/error.rs");
        let dep_files: Vec<&str> = deps.iter().map(|(p, _)| *p).collect();
        assert!(
            dep_files.contains(&"src/lib.rs"),
            "lib.rs should be a direct dependent (it imports from error.rs), got: {dep_files:?}"
        );
        assert!(
            dep_files.contains(&"src/main.rs"),
            "main.rs should be found via lib.rs pub use re-export, got: {dep_files:?}"
        );
    }

    #[test]
    fn test_find_dependents_non_pub_use_does_not_create_reexport_chain() {
        // Ensure that a normal `use` (not `pub use`) does NOT trigger
        // re-export chain resolution.

        let target = make_file_with_refs_and_content(
            "src/domain/index.rs",
            LanguageId::Rust,
            "pub struct Record {}",
            vec![],
            vec![SymbolRecord {
                name: "Record".to_string(),
                kind: SymbolKind::Struct,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 20),
                line_range: (0, 0),
                doc_byte_range: None,
                item_byte_range: None,
            }],
        );

        // mod.rs has private `use index::Record;` (not pub)
        let mod_content = "use index::Record;\nfn internal() {}";
        let private_importer = make_file_with_refs_and_content(
            "src/domain/mod.rs",
            LanguageId::Rust,
            mod_content,
            vec![make_ref(
                "Record",
                Some("index::Record"),
                ReferenceKind::Import,
                None,
                4, // byte offset of "index::Record" in "use index::Record;\n"
            )],
            vec![make_symbol("internal")],
        );

        // tools.rs imports from crate::domain but only mod.rs is the module root
        let consumer = make_file_with_refs_and_content(
            "src/tools.rs",
            LanguageId::Rust,
            "use crate::domain::Record;\n",
            vec![make_ref(
                "Record",
                Some("crate::domain::Record"),
                ReferenceKind::Import,
                None,
                0,
            )],
            vec![],
        );

        let index = make_index(
            vec![
                ("src/domain/index.rs", target),
                ("src/domain/mod.rs", private_importer),
                ("src/tools.rs", consumer),
            ],
            false,
        );

        let deps = index.find_dependents_for_file("src/domain/index.rs");
        let dep_files: Vec<&str> = deps.iter().map(|(p, _)| *p).collect();
        assert!(
            dep_files.contains(&"src/domain/mod.rs"),
            "mod.rs is a direct dependent, got: {dep_files:?}"
        );
        assert!(
            !dep_files.contains(&"src/tools.rs"),
            "tools.rs should NOT be found when mod.rs uses private `use` (not pub use), got: {dep_files:?}"
        );
    }

    #[test]
    fn test_find_dependents_qualified_call_without_import() {
        // A file that calls engine::optimize_deterministic() without
        // `use engine;` should still be a dependent of src/engine.rs.
        let target_sym =
            make_symbol_with_kind_and_line("optimize_deterministic", SymbolKind::Function, 10);
        let target_file = make_file_with_refs_and_content(
            "src/engine.rs",
            LanguageId::Rust,
            "pub fn optimize_deterministic() { todo!() }\n",
            vec![],
            vec![target_sym],
        );

        let call_ref = make_ref(
            "optimize_deterministic",
            Some("engine::optimize_deterministic"),
            ReferenceKind::Call,
            Some(0),
            100,
        );
        let caller_file =
            make_file_with_refs("src/ui/optimization.rs", vec![call_ref], HashMap::new());

        let index = make_index(
            vec![
                ("src/engine.rs", target_file),
                ("src/ui/optimization.rs", caller_file),
            ],
            false,
        );

        let deps = index.find_dependents_for_file("src/engine.rs");
        assert!(
            deps.iter()
                .any(|(path, _)| *path == "src/ui/optimization.rs"),
            "Should find qualified caller as dependent, got: {:?}",
            deps.iter().map(|(p, _)| p).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_resolve_module_path_rust_cases() {
        use super::resolve_module_path;
        assert_eq!(
            resolve_module_path("src/lib.rs", &LanguageId::Rust),
            Some("crate".to_string())
        );
        assert_eq!(
            resolve_module_path("src/main.rs", &LanguageId::Rust),
            Some("crate".to_string())
        );
        assert_eq!(
            resolve_module_path("src/error.rs", &LanguageId::Rust),
            Some("crate::error".to_string())
        );
        assert_eq!(
            resolve_module_path("src/live_index/mod.rs", &LanguageId::Rust),
            Some("crate::live_index".to_string())
        );
        assert_eq!(
            resolve_module_path("src/live_index/store.rs", &LanguageId::Rust),
            Some("crate::live_index::store".to_string())
        );
        // Files outside src/ have no module path
        assert_eq!(resolve_module_path("tests/foo.rs", &LanguageId::Rust), None);
        // Workspace crates: "crates/my-crate/src/types.rs" → "crate::types"
        assert_eq!(
            resolve_module_path("crates/aap-core/src/types.rs", &LanguageId::Rust),
            Some("crate::types".to_string())
        );
        assert_eq!(
            resolve_module_path("crates/aap-core/src/lib.rs", &LanguageId::Rust),
            Some("crate".to_string())
        );
        assert_eq!(
            resolve_module_path("crates/aap-core/src/domain/mod.rs", &LanguageId::Rust),
            Some("crate::domain".to_string())
        );
        // Boundary: no false positives on non-src paths
        assert_eq!(
            resolve_module_path("benches/foo.rs", &LanguageId::Rust),
            None,
            "benches/ should not resolve"
        );
        assert_eq!(
            resolve_module_path("my-src/lib.rs", &LanguageId::Rust),
            None,
            "my-src/ should not match /src/ component"
        );
    }

    #[test]
    fn test_resolve_module_path_python_cases() {
        use super::resolve_module_path;
        assert_eq!(
            resolve_module_path("utils/__init__.py", &LanguageId::Python),
            Some("utils".to_string())
        );
        assert_eq!(
            resolve_module_path("utils/helpers.py", &LanguageId::Python),
            Some("utils.helpers".to_string())
        );
    }

    #[test]
    fn test_resolve_module_path_js_cases() {
        use super::resolve_module_path;
        assert_eq!(
            resolve_module_path("src/utils/index.js", &LanguageId::JavaScript),
            Some("src/utils".to_string())
        );
        assert_eq!(
            resolve_module_path("src/utils/index.ts", &LanguageId::TypeScript),
            Some("src/utils".to_string())
        );
        assert_eq!(
            resolve_module_path("src/foo.js", &LanguageId::JavaScript),
            Some("src/foo".to_string())
        );
    }

    // --- callees_for_symbol ---

    #[test]
    fn test_callees_for_symbol_returns_enclosed_calls() {
        let refs = vec![
            make_ref("helper", None, ReferenceKind::Call, Some(0), 0),
            make_ref("other", None, ReferenceKind::Call, Some(1), 100), // different enclosing
            make_ref("imported", None, ReferenceKind::Import, Some(0), 200), // not a Call
        ];
        let f = make_file_with_refs("src/main.rs", refs, HashMap::new());
        let index = make_index(vec![("src/main.rs", f)], false);

        let callees = index.callees_for_symbol("src/main.rs", 0);
        assert_eq!(
            callees.len(),
            1,
            "only the Call reference with enclosing=0 should be returned"
        );
        assert_eq!(callees[0].name, "helper");
    }

    #[test]
    fn test_callees_for_symbol_includes_calls_inside_nested_methods() {
        let file = make_file_with_refs_and_content(
            "src/service.rs",
            LanguageId::CSharp,
            r#"public class MinioService {
    public async Task UploadAsync() {
        _minioClient.BucketExistsAsync();
        _logger.LogInformation(""upload"");
    }
}"#,
            vec![
                make_ref("BucketExistsAsync", None, ReferenceKind::Call, Some(1), 100),
                make_ref("LogInformation", None, ReferenceKind::Call, Some(1), 200),
            ],
            vec![
                SymbolRecord {
                    name: "MinioService".to_string(),
                    kind: SymbolKind::Class,
                    depth: 0,
                    sort_order: 0,
                    byte_range: (0, 500),
                    line_range: (0, 4),
                    doc_byte_range: None,
                    item_byte_range: None,
                },
                SymbolRecord {
                    name: "UploadAsync".to_string(),
                    kind: SymbolKind::Method,
                    depth: 1,
                    sort_order: 1,
                    byte_range: (50, 400),
                    line_range: (1, 3),
                    doc_byte_range: None,
                    item_byte_range: None,
                },
            ],
        );
        let index = make_index(vec![("src/service.rs", file)], false);

        let callees = index.callees_for_symbol("src/service.rs", 0);
        assert_eq!(
            callees.len(),
            2,
            "class bundles should surface calls made inside enclosed methods"
        );
        assert_eq!(callees[0].name, "BucketExistsAsync");
        assert_eq!(callees[1].name, "LogInformation");
    }

    #[test]
    fn test_callees_for_symbol_empty_for_nonexistent_file() {
        let index = make_index(vec![], false);
        let callees = index.callees_for_symbol("nonexistent.rs", 0);
        assert!(callees.is_empty(), "nonexistent file returns empty callees");
    }

    #[test]
    fn test_callees_for_symbol_excludes_non_call_kinds() {
        let refs = vec![
            make_ref("T", None, ReferenceKind::TypeUsage, Some(0), 0),
            make_ref("my_macro", None, ReferenceKind::MacroUse, Some(0), 50),
        ];
        let f = make_file_with_refs("src/lib.rs", refs, HashMap::new());
        let index = make_index(vec![("src/lib.rs", f)], false);

        let callees = index.callees_for_symbol("src/lib.rs", 0);
        assert!(
            callees.is_empty(),
            "TypeUsage and MacroUse should not appear in callees"
        );
    }

    // --- is_filtered_name (unit coverage) ---

    #[test]
    fn test_is_filtered_name_rust_builtins() {
        use super::is_filtered_name;
        assert!(
            is_filtered_name("i32", &LanguageId::Rust),
            "i32 is a Rust built-in"
        );
        assert!(
            is_filtered_name("bool", &LanguageId::Rust),
            "bool is a Rust built-in"
        );
        assert!(
            is_filtered_name("String", &LanguageId::Rust),
            "String is a Rust built-in"
        );
        assert!(
            !is_filtered_name("MyString", &LanguageId::Rust),
            "MyString is not a built-in"
        );
    }

    #[test]
    fn test_is_filtered_name_single_letter_generics() {
        use super::is_filtered_name;
        assert!(
            is_filtered_name("T", &LanguageId::Rust),
            "T is a single-letter generic"
        );
        assert!(
            is_filtered_name("K", &LanguageId::TypeScript),
            "K is a single-letter generic"
        );
        assert!(
            is_filtered_name("V", &LanguageId::Python),
            "V is a single-letter generic"
        );
        assert!(
            !is_filtered_name("Key", &LanguageId::Rust),
            "Key is not a single-letter generic"
        );
    }

    #[test]
    fn test_is_filtered_name_respects_language_scope() {
        use super::is_filtered_name;
        assert!(
            is_filtered_name("object", &LanguageId::Python),
            "Python built-ins should still be filtered in Python files"
        );
        assert!(
            !is_filtered_name("object", &LanguageId::Rust),
            "Python built-ins must not hide Rust identifiers"
        );
        assert!(
            is_filtered_name("Object", &LanguageId::TypeScript),
            "TypeScript built-ins should be filtered in TypeScript files"
        );
        assert!(
            !is_filtered_name("Object", &LanguageId::Rust),
            "TypeScript built-ins must not hide Rust type names"
        );
    }

    // --- is_vendor_path / is_personal_tooling_path (unit coverage) ---

    #[test]
    fn test_is_vendor_path_matches_vendor_dirs() {
        use super::is_vendor_path;
        assert!(is_vendor_path("vendor/tree-sitter-scss/src/parser.c"));
        assert!(is_vendor_path("third_party/foo/bar.rs"));
        assert!(is_vendor_path("node_modules/react/index.js"));
        assert!(!is_vendor_path("src/parsing/mod.rs"));
        assert!(!is_vendor_path("tests/vendor_smoke.rs")); // basename, not directory
    }

    #[test]
    fn test_is_personal_tooling_path_matches_claude_dirs() {
        use super::is_personal_tooling_path;
        assert!(is_personal_tooling_path(".claude/gsd-local-patches/foo.md"));
        assert!(is_personal_tooling_path(".claude/get-shit-done/bar.sh"));
        assert!(!is_personal_tooling_path(".claude/CLAUDE.md")); // root-level claude config
        assert!(!is_personal_tooling_path("src/lib.rs"));
    }

    #[test]
    fn test_is_personal_tooling_path_matches_obsidian_internals() {
        use super::is_personal_tooling_path;
        assert!(is_personal_tooling_path(".obsidian/app.json"));
        assert!(is_personal_tooling_path("wiki/.obsidian/workspace.json"));
        assert!(is_personal_tooling_path(
            ".obsidian/plugins/dataview/styles.css"
        ));
        assert!(is_personal_tooling_path(
            "wiki\\.obsidian\\plugins\\dataview\\styles.css"
        ));
    }

    #[test]
    fn test_is_personal_tooling_path_allows_normal_wiki_markdown() {
        use super::is_personal_tooling_path;
        assert!(!is_personal_tooling_path("wiki/notes.md"));
        assert!(!is_personal_tooling_path("wiki/.obsidian.md"));
    }

    #[test]
    fn test_is_vendor_path_matches_extended_components() {
        use super::is_vendor_path;
        // Option B delegates to NoisePolicy::classify_path which catches the
        // full 9-component set, not just plan's 4. Prove that here.
        assert!(is_vendor_path(".venv/lib/python3.11/site.py"));
        assert!(is_vendor_path("venv/lib/python3.11/site.py"));
        assert!(is_vendor_path("project/site-packages/foo.py"));
        assert!(is_vendor_path("ios/Pods/AFNetworking/foo.m"));
        assert!(is_vendor_path("frontend/bower_components/jquery/jquery.js"));
    }

    #[test]
    fn test_is_vendor_path_case_insensitive() {
        use super::is_vendor_path;
        assert!(is_vendor_path("Vendor/Foo.rs"));
        assert!(is_vendor_path("NODE_MODULES/react/index.js"));
    }

    #[test]
    fn test_is_vendor_path_windows_separator() {
        use super::is_vendor_path;
        assert!(is_vendor_path("vendor\\foo\\bar.rs"));
        assert!(is_vendor_path("project\\node_modules\\react\\index.js"));
    }

    #[test]
    fn test_is_personal_tooling_path_matches_gsd_variant() {
        use super::is_personal_tooling_path;
        assert!(is_personal_tooling_path(".claude/gsd-something-new/x.md"));
        assert!(is_personal_tooling_path(
            ".claude/gsd-anything/nested/file.sh"
        ));
    }

    #[test]
    fn test_is_personal_tooling_path_excludes_claude_commands() {
        use super::is_personal_tooling_path;
        assert!(!is_personal_tooling_path(".claude/commands/foo.sh"));
        assert!(!is_personal_tooling_path(".claude/skills/x/SKILL.md"));
        assert!(!is_personal_tooling_path(".claude/hooks/run.py"));
        assert!(!is_personal_tooling_path(".claude/agents/explore.md"));
    }

    #[test]
    fn test_callees_for_symbol_keeps_cross_language_builtin_name_in_rust() {
        let file = make_file_with_refs_and_content(
            "src/lib.rs",
            LanguageId::Rust,
            "fn handler() { Object(); }",
            vec![make_ref("Object", None, ReferenceKind::Call, Some(0), 0)],
            vec![SymbolRecord {
                name: "handler".to_string(),
                kind: SymbolKind::Function,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 25),
                line_range: (0, 0),
                item_byte_range: Some((0, 25)),
                doc_byte_range: None,
            }],
        );
        let index = make_index(vec![("src/lib.rs", file)], false);

        let callees = index.callees_for_symbol("src/lib.rs", 0);
        assert_eq!(
            callees.len(),
            1,
            "Rust callees should not use Python/TS filters"
        );
        assert_eq!(callees[0].name, "Object");
    }

    #[test]
    fn test_resolve_type_dependencies_keeps_cross_language_builtin_name_in_rust() {
        let object_file = make_file_with_refs_and_content(
            "src/model.rs",
            LanguageId::Rust,
            "struct Object;",
            vec![],
            vec![SymbolRecord {
                name: "Object".to_string(),
                kind: SymbolKind::Struct,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 14),
                line_range: (0, 0),
                item_byte_range: Some((0, 14)),
                doc_byte_range: None,
            }],
        );
        let index = make_index(vec![("src/model.rs", object_file)], false);

        let deps = index.resolve_type_dependencies(&["Object"], 0);
        assert_eq!(
            deps.len(),
            1,
            "Rust type names should survive non-Rust builtin filters"
        );
        assert_eq!(deps[0].name, "Object");
    }

    #[test]
    fn test_type_refs_for_symbol_returns_type_usages_within_symbol() {
        let refs = vec![
            make_ref("UserConfig", None, ReferenceKind::TypeUsage, Some(0), 0),
            make_ref("helper", None, ReferenceKind::Call, Some(0), 50),
            make_ref("Address", None, ReferenceKind::TypeUsage, Some(1), 100),
        ];
        let file = make_file_with_refs_and_content(
            "src/main.rs",
            LanguageId::Rust,
            "fn process(cfg: UserConfig) { helper(); }\nfn other(a: Address) {}",
            refs,
            vec![
                SymbolRecord {
                    name: "process".to_string(),
                    kind: SymbolKind::Function,
                    depth: 0,
                    sort_order: 0,
                    byte_range: (0, 40),
                    line_range: (0, 0),
                    doc_byte_range: None,
                    item_byte_range: None,
                },
                SymbolRecord {
                    name: "other".to_string(),
                    kind: SymbolKind::Function,
                    depth: 0,
                    sort_order: 1,
                    byte_range: (41, 65),
                    line_range: (1, 1),
                    doc_byte_range: None,
                    item_byte_range: None,
                },
            ],
        );
        let index = make_index(vec![("src/main.rs", file)], false);

        let type_refs = index.type_refs_for_symbol("src/main.rs", 0);
        assert_eq!(type_refs.len(), 1, "only TypeUsage within symbol 0");
        assert_eq!(type_refs[0].name, "UserConfig");
    }

    #[test]
    fn test_resolve_type_dependencies_finds_struct_definitions() {
        let config_body = "pub struct UserConfig {\n    pub name: String,\n}";
        let config_file = make_file_with_refs_and_content(
            "src/config.rs",
            LanguageId::Rust,
            config_body,
            vec![],
            vec![SymbolRecord {
                name: "UserConfig".to_string(),
                kind: SymbolKind::Struct,
                depth: 0,
                sort_order: 0,
                byte_range: (0, config_body.len() as u32),
                line_range: (0, 2),
                doc_byte_range: None,
                item_byte_range: None,
            }],
        );
        let index = make_index(vec![("src/config.rs", config_file)], false);

        let deps = index.resolve_type_dependencies(&["UserConfig", "String", "T"], 0);
        assert_eq!(deps.len(), 1, "String and T should be filtered out");
        assert_eq!(deps[0].name, "UserConfig");
        assert_eq!(deps[0].kind_label, "struct");
        assert_eq!(deps[0].file_path, "src/config.rs");
        assert!(deps[0].body.contains("pub struct UserConfig"));
    }

    #[test]
    fn test_resolve_type_dependencies_recurses_to_depth() {
        let addr_body = "pub struct Address {\n    pub city: String,\n}";
        let config_body = "pub struct UserConfig {\n    pub addr: Address,\n}";
        let config_file = make_file_with_refs_and_content(
            "src/config.rs",
            LanguageId::Rust,
            config_body,
            vec![ReferenceRecord {
                name: "Address".to_string(),
                qualified_name: None,
                kind: ReferenceKind::TypeUsage,
                byte_range: (30, 37),
                line_range: (1, 1),
                enclosing_symbol_index: Some(0),
            }],
            vec![SymbolRecord {
                name: "UserConfig".to_string(),
                kind: SymbolKind::Struct,
                depth: 0,
                sort_order: 0,
                byte_range: (0, config_body.len() as u32),
                line_range: (0, 1),
                doc_byte_range: None,
                item_byte_range: None,
            }],
        );
        let addr_file = make_file_with_refs_and_content(
            "src/address.rs",
            LanguageId::Rust,
            addr_body,
            vec![],
            vec![SymbolRecord {
                name: "Address".to_string(),
                kind: SymbolKind::Struct,
                depth: 0,
                sort_order: 0,
                byte_range: (0, addr_body.len() as u32),
                line_range: (0, 1),
                doc_byte_range: None,
                item_byte_range: None,
            }],
        );
        let index = make_index(
            vec![
                ("src/config.rs", config_file),
                ("src/address.rs", addr_file),
            ],
            false,
        );

        // Depth 0: only UserConfig, no recursion.
        let deps_d0 = index.resolve_type_dependencies(&["UserConfig"], 0);
        assert_eq!(deps_d0.len(), 1, "depth 0 should only find UserConfig");

        // Depth 1: UserConfig + Address (found transitively).
        let deps_d1 = index.resolve_type_dependencies(&["UserConfig"], 1);
        assert_eq!(
            deps_d1.len(),
            2,
            "depth 1 should find UserConfig and Address"
        );
        let names: Vec<&str> = deps_d1.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"UserConfig"));
        assert!(names.contains(&"Address"));
        // Verify depth markers.
        let uc = deps_d1.iter().find(|d| d.name == "UserConfig").unwrap();
        let ad = deps_d1.iter().find(|d| d.name == "Address").unwrap();
        assert_eq!(uc.depth, 0);
        assert_eq!(ad.depth, 1);
    }

    #[test]
    fn test_resolve_type_dependencies_respects_max_cap() {
        // Create 20 distinct struct types — should be capped at 15.
        let files: Vec<(&str, IndexedFile)> = (0..20)
            .map(|i| {
                let name = format!("Type{i}");
                let body = format!("pub struct {name} {{}}");
                let leaked_path: &'static str = Box::leak(format!("src/t{i}.rs").into_boxed_str());
                let leaked_body: &'static str = Box::leak(body.into_boxed_str());
                let f = make_file_with_refs_and_content(
                    leaked_path,
                    LanguageId::Rust,
                    leaked_body,
                    vec![],
                    vec![SymbolRecord {
                        name: name.clone(),
                        kind: SymbolKind::Struct,
                        depth: 0,
                        sort_order: 0,
                        byte_range: (0, leaked_body.len() as u32),
                        line_range: (0, 0),
                        doc_byte_range: None,
                        item_byte_range: None,
                    }],
                );
                (leaked_path, f)
            })
            .collect();
        let index = make_index(files, false);

        let type_names: Vec<&str> = (0..20)
            .map(|i| {
                let s: &'static str = Box::leak(format!("Type{i}").into_boxed_str());
                s
            })
            .collect();
        let deps = index.resolve_type_dependencies(&type_names, 0);
        assert!(
            deps.len() <= 15,
            "should be capped at MAX_DEPENDENCIES=15, got {}",
            deps.len()
        );
    }

    // -----------------------------------------------------------------------
    // kind_disambiguation_tier + resolve_symbol_selector tier tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_kind_disambiguation_tier_values() {
        use super::kind_disambiguation_tier;
        assert_eq!(kind_disambiguation_tier(&SymbolKind::Class), 1);
        assert_eq!(kind_disambiguation_tier(&SymbolKind::Struct), 1);
        assert_eq!(kind_disambiguation_tier(&SymbolKind::Enum), 1);
        assert_eq!(kind_disambiguation_tier(&SymbolKind::Interface), 1);
        assert_eq!(kind_disambiguation_tier(&SymbolKind::Trait), 1);
        assert_eq!(kind_disambiguation_tier(&SymbolKind::Module), 2);
        assert_eq!(kind_disambiguation_tier(&SymbolKind::Function), 3);
        assert_eq!(kind_disambiguation_tier(&SymbolKind::Method), 3);
        assert_eq!(kind_disambiguation_tier(&SymbolKind::Impl), 3);
        assert_eq!(kind_disambiguation_tier(&SymbolKind::Constant), 4);
        assert_eq!(kind_disambiguation_tier(&SymbolKind::Variable), 4);
        assert_eq!(kind_disambiguation_tier(&SymbolKind::Type), 4);
        assert_eq!(kind_disambiguation_tier(&SymbolKind::Key), 4);
        assert_eq!(kind_disambiguation_tier(&SymbolKind::Section), 4);
        assert_eq!(kind_disambiguation_tier(&SymbolKind::Other), 4);
    }

    #[test]
    fn test_resolve_selector_class_vs_constructor_returns_class() {
        // SYMB-02: C# class "Foo" + constructor "Foo" (mapped to Function)
        use super::{SymbolSelectorMatch, resolve_symbol_selector};
        let class_sym = SymbolRecord {
            kind: SymbolKind::Class,
            line_range: (0, 20),
            ..make_symbol("Foo")
        };
        let ctor_sym = SymbolRecord {
            kind: SymbolKind::Function,
            line_range: (5, 15),
            ..make_symbol("Foo")
        };
        let file = make_indexed_file("src/Foo.cs", vec![class_sym, ctor_sym], ParseStatus::Parsed);
        match resolve_symbol_selector(&file, "Foo", None, None) {
            SymbolSelectorMatch::Selected(idx, sym) => {
                assert_eq!(idx, 0);
                assert_eq!(sym.kind, SymbolKind::Class);
            }
            other => panic!(
                "Expected Selected, got {:?}",
                match other {
                    SymbolSelectorMatch::Ambiguous(v) => format!("Ambiguous({v:?})"),
                    SymbolSelectorMatch::NotFound => "NotFound".to_string(),
                    _ => unreachable!(),
                }
            ),
        }
    }

    #[test]
    fn test_resolve_selector_module_vs_function_returns_module() {
        // Cross-tier: module (tier 2) beats function (tier 3)
        use super::{SymbolSelectorMatch, resolve_symbol_selector};
        let mod_sym = SymbolRecord {
            kind: SymbolKind::Module,
            line_range: (0, 50),
            ..make_symbol("utils")
        };
        let fn_sym = SymbolRecord {
            kind: SymbolKind::Function,
            line_range: (10, 20),
            ..make_symbol("utils")
        };
        let file = make_indexed_file("src/utils.py", vec![mod_sym, fn_sym], ParseStatus::Parsed);
        match resolve_symbol_selector(&file, "utils", None, None) {
            SymbolSelectorMatch::Selected(idx, sym) => {
                assert_eq!(idx, 0);
                assert_eq!(sym.kind, SymbolKind::Module);
            }
            other => panic!(
                "Expected Selected for Module, got {:?}",
                match other {
                    SymbolSelectorMatch::Ambiguous(v) => format!("Ambiguous({v:?})"),
                    SymbolSelectorMatch::NotFound => "NotFound".to_string(),
                    _ => unreachable!(),
                }
            ),
        }
    }

    #[test]
    fn test_resolve_selector_same_tier_returns_ambiguous() {
        // SYMB-03: Two functions with same name = same tier = Ambiguous
        use super::{SymbolSelectorMatch, resolve_symbol_selector};
        let fn1 = SymbolRecord {
            kind: SymbolKind::Function,
            line_range: (0, 5),
            ..make_symbol("connect")
        };
        let fn2 = SymbolRecord {
            kind: SymbolKind::Function,
            line_range: (10, 15),
            ..make_symbol("connect")
        };
        let file = make_indexed_file("src/db.rs", vec![fn1, fn2], ParseStatus::Parsed);
        match resolve_symbol_selector(&file, "connect", None, None) {
            SymbolSelectorMatch::Ambiguous(lines) => {
                assert_eq!(lines, vec![0, 10]);
            }
            _ => panic!("Expected Ambiguous for same-tier symbols"),
        }
    }

    #[test]
    fn test_resolve_selector_same_tier_class_struct_returns_ambiguous() {
        // SYMB-03: Class + Struct at tier 1 = Ambiguous
        use super::{SymbolSelectorMatch, resolve_symbol_selector};
        let class_sym = SymbolRecord {
            kind: SymbolKind::Class,
            line_range: (0, 10),
            ..make_symbol("Data")
        };
        let struct_sym = SymbolRecord {
            kind: SymbolKind::Struct,
            line_range: (20, 30),
            ..make_symbol("Data")
        };
        let file = make_indexed_file(
            "src/data.rs",
            vec![class_sym, struct_sym],
            ParseStatus::Parsed,
        );
        match resolve_symbol_selector(&file, "Data", None, None) {
            SymbolSelectorMatch::Ambiguous(lines) => {
                assert_eq!(lines, vec![0, 20]);
            }
            _ => panic!("Expected Ambiguous for two tier-1 symbols"),
        }
    }

    #[test]
    fn test_resolve_selector_explicit_kind_bypasses_tier_logic() {
        // When symbol_kind is specified, tier logic should NOT apply
        use super::{SymbolSelectorMatch, resolve_symbol_selector};
        let class_sym = SymbolRecord {
            kind: SymbolKind::Class,
            line_range: (0, 20),
            ..make_symbol("Foo")
        };
        let fn_sym = SymbolRecord {
            kind: SymbolKind::Function,
            line_range: (5, 15),
            ..make_symbol("Foo")
        };
        let file = make_indexed_file("src/Foo.cs", vec![class_sym, fn_sym], ParseStatus::Parsed);
        // Asking specifically for "fn" should return the function, not the class
        match resolve_symbol_selector(&file, "Foo", Some("fn"), None) {
            SymbolSelectorMatch::Selected(idx, sym) => {
                assert_eq!(idx, 1);
                assert_eq!(sym.kind, SymbolKind::Function);
            }
            _ => panic!("Expected Selected for explicit kind filter"),
        }
    }

    #[test]
    fn test_resolve_selector_three_way_picks_highest_tier() {
        // Class (tier 1) + Module (tier 2) + Function (tier 3) => Class wins
        use super::{SymbolSelectorMatch, resolve_symbol_selector};
        let fn_sym = SymbolRecord {
            kind: SymbolKind::Function,
            line_range: (30, 40),
            ..make_symbol("Foo")
        };
        let mod_sym = SymbolRecord {
            kind: SymbolKind::Module,
            line_range: (0, 50),
            ..make_symbol("Foo")
        };
        let class_sym = SymbolRecord {
            kind: SymbolKind::Class,
            line_range: (10, 25),
            ..make_symbol("Foo")
        };
        // Intentionally not in tier order to test sorting
        let file = make_indexed_file(
            "src/Foo.cs",
            vec![fn_sym, mod_sym, class_sym],
            ParseStatus::Parsed,
        );
        match resolve_symbol_selector(&file, "Foo", None, None) {
            SymbolSelectorMatch::Selected(idx, sym) => {
                assert_eq!(idx, 2); // class_sym is at index 2 in the vec
                assert_eq!(sym.kind, SymbolKind::Class);
            }
            other => panic!(
                "Expected Selected for Class in 3-way, got {:?}",
                match other {
                    SymbolSelectorMatch::Ambiguous(v) => format!("Ambiguous({v:?})"),
                    SymbolSelectorMatch::NotFound => "NotFound".to_string(),
                    _ => unreachable!(),
                }
            ),
        }
    }

    #[test]
    fn test_qualified_name_suffix_match() {
        // "engine::optimize" should match module path "crate::engine" + target "optimize"
        assert!(super::matches_exact_symbol_qualified_name(
            &LanguageId::Rust,
            "engine::optimize",
            "optimize",
            Some("crate::engine"),
        ));
    }

    #[test]
    fn test_qualified_name_exact_match() {
        assert!(super::matches_exact_symbol_qualified_name(
            &LanguageId::Rust,
            "crate::engine::optimize",
            "optimize",
            Some("crate::engine"),
        ));
    }

    #[test]
    fn test_qualified_name_no_false_positive_on_partial_segment() {
        // "ngine::optimize" should NOT match "crate::engine" — partial segment
        assert!(!super::matches_exact_symbol_qualified_name(
            &LanguageId::Rust,
            "ngine::optimize",
            "optimize",
            Some("crate::engine"),
        ));
    }

    #[test]
    fn test_qualified_name_deep_module_suffix() {
        // "engine::sub::func" matching "crate::engine::sub" + "func"
        assert!(super::matches_exact_symbol_qualified_name(
            &LanguageId::Rust,
            "engine::sub::func",
            "func",
            Some("crate::engine::sub"),
        ));
    }

    #[test]
    fn test_collect_exact_refs_finds_qualified_call_without_import() {
        // Simulates: optimization.rs calls engine::optimize_deterministic()
        // but has no `use engine;` import — only a fully-qualified call.
        let target_sym =
            make_symbol_with_kind_and_line("optimize_deterministic", SymbolKind::Function, 10);
        let target_file = make_file_with_refs_and_content(
            "src/engine.rs",
            LanguageId::Rust,
            "pub fn optimize_deterministic() { todo!() }\n",
            vec![],
            vec![target_sym],
        );

        // Caller file: has a Call ref with qualified_name but NO import ref for "engine"
        let call_ref = make_ref(
            "optimize_deterministic",
            Some("engine::optimize_deterministic"),
            ReferenceKind::Call,
            Some(0),
            100,
        );
        let caller_file =
            make_file_with_refs("src/ui/optimization.rs", vec![call_ref], HashMap::new());

        let index = make_index(
            vec![
                ("src/engine.rs", target_file),
                ("src/ui/optimization.rs", caller_file),
            ],
            false,
        );

        let target = index.get_file("src/engine.rs").unwrap();
        let refs = index.collect_exact_symbol_references(
            "src/engine.rs",
            target,
            &target.symbols[0],
            None,
        );

        // Should find the caller even without a `use engine` import
        assert!(
            refs.iter()
                .any(|(path, _)| *path == "src/ui/optimization.rs"),
            "Should find caller in optimization.rs, got refs from: {:?}",
            refs.iter().map(|(p, _)| p).collect::<Vec<_>>()
        );
    }
}
