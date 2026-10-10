//! Read-oriented MCP tool input types and pure request parsing helpers.

use schemars::{JsonSchema, Schema};
use serde::{Deserialize, Deserializer, Serialize};

pub(crate) use crate::index_lifecycle::guidance::serde_input::{
    lenient_bool, lenient_option_vec, lenient_u32, lenient_u64,
};
pub(crate) fn lenient_bool_required<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<bool, D::Error> {
    lenient_bool(deserializer).map(|value| value.unwrap_or(false))
}
const INCLUDE_TESTS_SECTION_MARKER: &str = "__symforge_include_tests";

pub(crate) fn encode_include_tests_marker(
    mut sections: Option<Vec<String>>,
    include_tests: bool,
) -> Option<Vec<String>> {
    if include_tests {
        sections
            .get_or_insert_with(Vec::new)
            .push(INCLUDE_TESTS_SECTION_MARKER.to_string());
    }
    sections
}

pub(crate) fn include_tests_from_sections(sections: Option<&Vec<String>>) -> bool {
    sections
        .map(|items| {
            items
                .iter()
                .any(|section| section == INCLUDE_TESTS_SECTION_MARKER)
        })
        .unwrap_or(false)
}

pub(crate) fn visible_sections(sections: &Option<Vec<String>>) -> Option<Vec<String>> {
    sections.as_ref().and_then(|items| {
        let had_marker = items
            .iter()
            .any(|section| section.as_str() == INCLUDE_TESTS_SECTION_MARKER);
        let visible = items
            .iter()
            .filter(|section| section.as_str() != INCLUDE_TESTS_SECTION_MARKER)
            .cloned()
            .collect::<Vec<_>>();
        if visible.is_empty() && had_marker {
            None
        } else {
            Some(visible)
        }
    })
}

fn add_include_tests_schema(schema: &mut Schema) {
    if let Some(serde_json::Value::Object(properties)) = schema.get_mut("properties") {
        properties.insert(
            "include_tests".to_string(),
            serde_json::json!({
                "type": "boolean",
                "default": false,
                "description": "Include expanded test modules in context output."
            }),
        );
    }
}

pub(crate) fn lenient_u32_required<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<u32, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NumOrStr {
        Num(u32),
        Str(String),
    }
    match NumOrStr::deserialize(deserializer)? {
        NumOrStr::Num(n) => Ok(n),
        NumOrStr::Str(s) => s.parse::<u32>().map_err(serde::de::Error::custom),
    }
}

/// Deserialize an `i64` from either a JSON number or a stringified number.
pub(crate) fn lenient_i64<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<i64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NumOrStr {
        Num(i64),
        Str(String),
        Null,
    }
    match NumOrStr::deserialize(deserializer)? {
        NumOrStr::Num(n) => Ok(Some(n)),
        NumOrStr::Str(s) if s.is_empty() => Ok(None),
        NumOrStr::Str(s) => s.parse::<i64>().map(Some).map_err(serde::de::Error::custom),
        NumOrStr::Null => Ok(None),
    }
}

/// Leniently deserialize a `Vec<T>` — accepts a native JSON array, a stringified
/// JSON array (as sent by some MCP clients like Kilo Code), or a native array of
/// stringified JSON objects (as sent by Codex, e.g. `["{...}", "{...}"]`).
pub(crate) fn lenient_vec_required<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum VecOrStr {
        /// Native JSON array — try direct deserialization first via raw Value.
        Vec(Vec<serde_json::Value>),
        /// Entire array serialized as a single JSON string.
        Str(String),
    }
    match VecOrStr::deserialize(deserializer)? {
        VecOrStr::Vec(values) => {
            // Try deserializing each element. If an element is a JSON string
            // that looks like a JSON object, parse the inner string first.
            values
                .into_iter()
                .enumerate()
                .map(|(i, v)| {
                    // First try direct deserialization (normal case: native objects)
                    match serde_json::from_value::<T>(v.clone()) {
                        Ok(item) => Ok(item),
                        Err(direct_err) => {
                            // If the value is a string, try parsing it as JSON
                            if let serde_json::Value::String(ref s) = v {
                                serde_json::from_str::<T>(s).map_err(|_| {
                                    serde::de::Error::custom(format!("element {i}: {direct_err}"))
                                })
                            } else {
                                Err(serde::de::Error::custom(format!(
                                    "element {i}: {direct_err}"
                                )))
                            }
                        }
                    }
                })
                .collect()
        }
        VecOrStr::Str(s) => serde_json::from_str::<Vec<T>>(&s).map_err(serde::de::Error::custom),
    }
}

/// Input for `get_symbol`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct GetSymbolInput {
    /// Optional open-project ID or unique name (daemon). Omit for the active
    /// project; local/embedded servers refuse non-matching selectors.
    #[serde(default)]
    pub project: Option<String>,
    /// Optional file path. If omitted, resolve `name` by exact index search; use
    /// `symbol_line`/`kind` to disambiguate. If set, open that file directly.
    /// Ignored when `targets` is provided.
    #[serde(default)]
    pub path: String,
    /// Symbol name to look up (required for single lookup; ignored when `targets` is provided).
    #[serde(default)]
    pub name: String,
    /// Optional kind filter: "fn", "struct", "enum", "impl", etc.
    pub kind: Option<String>,
    /// Disambiguate when multiple symbols share the same name. Pass the 1-based start line of the
    /// desired symbol (shown in ambiguity errors and search_symbols output).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub symbol_line: Option<u32>,
    /// Optional batch mode: provide multiple targets to retrieve 2+ symbols or code slices in one call.
    /// Each target is a file path + symbol name or byte range. When provided, path/name/kind above are ignored.
    #[serde(default, deserialize_with = "lenient_option_vec")]
    #[schemars(with = "Vec<SymbolTarget>")]
    pub targets: Option<Vec<SymbolTarget>>,
    /// When true, estimate output tokens before fetching large symbols.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub estimate: Option<bool>,
    /// Response token budget; defaults to about 1,000 when unset.
    #[serde(default)]
    pub max_tokens: Option<u64>,
    /// When true, bypass session cache-hit and return a fresh payload.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub force_refresh: Option<bool>,
}

/// A single target in a `get_symbols` batch request.
///
/// Either provide `name` (symbol lookup) or `start_byte`/`end_byte` (code slice).
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct SymbolTarget {
    /// Relative file path.
    pub path: String,
    /// Symbol name for symbol lookup (mutually exclusive with byte range).
    pub name: Option<String>,
    /// Kind filter for symbol lookup (e.g., "fn", "struct").
    pub kind: Option<String>,
    /// Disambiguate when multiple symbols share the same name. Pass the 1-based start line.
    #[serde(default, deserialize_with = "lenient_u32")]
    pub symbol_line: Option<u32>,
    /// Start byte offset for code slice (mutually exclusive with name).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub start_byte: Option<u32>,
    /// End byte offset for code slice (inclusive).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub end_byte: Option<u32>,
}

/// Input for `get_file_content`.
pub use crate::index_lifecycle::guidance::read_contract::GetFileContentInput;

/// Input for `symforge_retrieve` (CCR full output recovery, 011).
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct SymforgeRetrieveInput {
    /// 12-character hex handle from a CCR footer.
    pub hash: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub(crate) struct ValidateFileSyntaxInput {
    /// Optional open-project ID or unique name (daemon). Omit for the active
    /// project; local/embedded servers refuse non-matching selectors.
    #[serde(default)]
    pub project: Option<String>,
    pub path: String,
}

/// Input for `find_dependents`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct FindDependentsInput {
    /// Optional open-project ID or unique name (daemon). Omit for the active
    /// project; local/embedded servers refuse non-matching selectors.
    #[serde(default)]
    pub project: Option<String>,
    /// Relative file path to find dependents for.
    pub path: String,
    /// Optional symbol name, accepted only to catch misuse: with `path`, it
    /// redirects to `find_references` (symbol callers). Omit it for the file-level
    /// graph of files that import `path`.
    #[serde(default)]
    pub name: Option<String>,
    /// Maximum number of dependent files to show (default 20, capped at 100).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub limit: Option<u32>,
    /// Maximum number of reference lines per file (default 5, capped at 50).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub max_per_file: Option<u32>,
    /// Output format: "text" (default), "mermaid", or "dot".
    pub format: Option<String>,
    /// When true, show compact output: one line per dependent file as
    /// `path (N refs: M call, K type_usage, J import)` with no source text
    /// (60-75% smaller). Best for hub files with many dependents.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub compact: Option<bool>,
    /// When true, estimate token cost instead of returning content.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub estimate: Option<bool>,
    /// Response token budget.
    #[serde(default, deserialize_with = "lenient_u64")]
    pub max_tokens: Option<u64>,
}

/// Input for `get_repo_map`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct GetRepoMapInput {
    /// Optional open-project ID or unique name (daemon). Omit for the active
    /// project; local/embedded servers refuse non-matching selectors.
    #[serde(default)]
    pub project: Option<String>,
    /// Detail level: "compact" (default — ~500 token project overview), "full" (complete symbol outline of every file), "tree" (browsable file tree with per-file stats).
    pub detail: Option<String>,
    /// Subtree path to browse (only used when detail="tree", default: project root).
    pub path: Option<String>,
    /// Max depth levels to expand (only used when detail="tree", default: 2, max: 5).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub depth: Option<u32>,
    /// Maximum number of files to include in the output (only used when detail="full", default: 200).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub max_files: Option<u32>,
    /// When true, estimate token cost instead of returning content.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub estimate: Option<bool>,
    /// Response token budget.
    #[serde(default, deserialize_with = "lenient_u64")]
    pub max_tokens: Option<u64>,
}

/// Input for `get_file_context`.
#[derive(Serialize, JsonSchema)]
#[schemars(transform = add_file_context_schema)]
pub struct GetFileContextInput {
    /// Optional open-project ID or unique name (daemon). Omit for the active
    /// project; local/embedded servers refuse non-matching selectors.
    #[serde(default)]
    pub project: Option<String>,
    /// Relative path to the file.
    pub path: String,
    /// Optional max token budget, matching hook behavior.
    #[serde(default, deserialize_with = "lenient_u64")]
    pub max_tokens: Option<u64>,
    /// Optional list of sections to include. Allowed values: "outline", "imports", "consumers", "references", "git", "knowledge". Omit or pass an empty list to include all sections.
    #[serde(default, deserialize_with = "lenient_option_vec")]
    #[schemars(with = "Vec<String>")]
    pub sections: Option<Vec<String>>,
    /// When true, estimate token cost instead of returning content.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub estimate: Option<bool>,
    /// When true, bypass session cache-hit and return a fresh payload.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub force_refresh: Option<bool>,
}

impl<'de> Deserialize<'de> for GetFileContextInput {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Raw {
            #[serde(default)]
            project: Option<String>,
            path: String,
            #[serde(default, deserialize_with = "lenient_u64")]
            max_tokens: Option<u64>,
            #[serde(default, deserialize_with = "lenient_option_vec")]
            sections: Option<Vec<String>>,
            #[serde(default, deserialize_with = "lenient_bool_required")]
            include_tests: bool,
            #[serde(default, deserialize_with = "lenient_bool")]
            estimate: Option<bool>,
            #[serde(default, deserialize_with = "lenient_bool")]
            force_refresh: Option<bool>,
        }

        let raw = Raw::deserialize(deserializer)?;
        Ok(Self {
            project: raw.project,
            path: raw.path,
            max_tokens: raw.max_tokens,
            sections: encode_include_tests_marker(raw.sections, raw.include_tests),
            estimate: raw.estimate,
            force_refresh: raw.force_refresh,
        })
    }
}

/// Input for `get_symbol_context`.
#[derive(Serialize, JsonSchema)]
#[schemars(transform = add_symbol_context_schema)]
pub struct GetSymbolContextInput {
    /// Optional open-project ID or unique name (daemon). Omit for the active
    /// project; local/embedded servers refuse non-matching selectors.
    #[serde(default)]
    pub project: Option<String>,
    /// Symbol name to inspect.
    pub name: String,
    /// Optional file filter (ignored when bundle=true; use path instead).
    pub file: Option<String>,
    /// File path from `search_symbols`. Required when bundle=true or sections is provided.
    pub path: Option<String>,
    /// Optional selected symbol kind such as `fn`, `class`, or `struct`.
    pub symbol_kind: Option<String>,
    /// Optional selected symbol line from `search_symbols`.
    #[serde(default, deserialize_with = "lenient_u32")]
    pub symbol_line: Option<u32>,
    /// Output verbosity: `summary` (one-line, ~90% smaller), `signature`
    /// (name/params/return, ~80% smaller), `compact` (signature + first doc line),
    /// or `full` (complete body; default). Applies to default mode's definition,
    /// bundle mode's main symbol (dependency types stay full), and the definition
    /// in a sections/trace header.
    pub verbosity: Option<String>,
    /// When true, switch to bundle mode: returns symbol body + full definitions of all referenced custom types, resolved recursively. Best for edit preparation. Requires path.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub bundle: Option<bool>,
    /// When provided, switches to trace mode (definition, callers, callees,
    /// implementations, type dependencies, git activity). Values: `dependents`,
    /// `siblings`, `implementations`, `git`, `knowledge`. Omit for default mode;
    /// an empty array selects all trace sections.
    #[serde(default, deserialize_with = "lenient_option_vec")]
    #[schemars(with = "Vec<String>")]
    pub sections: Option<Vec<String>>,
    /// Bundle token budget. Preserve the main body and sections, then include
    /// direct before transitive type dependencies until exhausted (~4 chars/token).
    #[serde(default, deserialize_with = "lenient_u64")]
    pub max_tokens: Option<u64>,
    /// When true, estimate output tokens for the body, callers, bundle, and raw file.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub estimate: Option<bool>,
}

impl<'de> Deserialize<'de> for GetSymbolContextInput {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Raw {
            #[serde(default)]
            project: Option<String>,
            name: String,
            file: Option<String>,
            path: Option<String>,
            symbol_kind: Option<String>,
            #[serde(default, deserialize_with = "lenient_u32")]
            symbol_line: Option<u32>,
            verbosity: Option<String>,
            #[serde(default, deserialize_with = "lenient_bool")]
            bundle: Option<bool>,
            #[serde(default, deserialize_with = "lenient_option_vec")]
            sections: Option<Vec<String>>,
            #[serde(default, deserialize_with = "lenient_bool_required")]
            include_tests: bool,
            #[serde(default, deserialize_with = "lenient_u64")]
            max_tokens: Option<u64>,
            #[serde(default, deserialize_with = "lenient_bool")]
            estimate: Option<bool>,
        }

        let raw = Raw::deserialize(deserializer)?;
        let sections = match raw.sections {
            Some(sections) => encode_include_tests_marker(Some(sections), raw.include_tests),
            None => None,
        };
        Ok(Self {
            project: raw.project,
            name: raw.name,
            file: raw.file,
            path: raw.path,
            symbol_kind: raw.symbol_kind,
            symbol_line: raw.symbol_line,
            verbosity: raw.verbosity,
            bundle: raw.bundle,
            sections,
            max_tokens: raw.max_tokens,
            estimate: raw.estimate,
        })
    }
}

/// Input for `trace_symbol`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct TraceSymbolInput {
    /// File path containing the symbol.
    pub path: String,
    /// Symbol name to trace.
    pub name: String,
    /// Optional kind filter (e.g., "fn", "struct").
    pub kind: Option<String>,
    /// Optional line number to disambiguate overloaded symbols.
    #[serde(default, deserialize_with = "lenient_u32")]
    pub symbol_line: Option<u32>,
    /// Optional list of output sections to include. When omitted, all sections are included.
    /// Valid values: "dependents", "siblings", "implementations", "git".
    #[serde(default, deserialize_with = "lenient_option_vec")]
    pub sections: Option<Vec<String>>,
    /// Output verbosity: "summary" (one-line natural language summary ~90% smaller), "signature" (name+params+return only, ~80% smaller), "compact" (signature + first doc line), "full" (default — complete body).
    pub verbosity: Option<String>,
    /// Response token budget.
    #[serde(default, deserialize_with = "lenient_u64")]
    pub max_tokens: Option<u64>,
    /// When true, estimate token cost instead of returning content.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub estimate: Option<bool>,
}

/// Input for `inspect_match`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct InspectMatchInput {
    /// Relative path to the file.
    pub path: String,
    /// 1-based line number to inspect.
    #[serde(deserialize_with = "lenient_u32_required")]
    pub line: u32,
    /// Number of context lines to show around the match (default 3).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub context: Option<u32>,
    /// Maximum number of siblings to show (default 10). Use 0 to hide siblings entirely.
    #[serde(default, deserialize_with = "lenient_u32")]
    pub sibling_limit: Option<u32>,
    /// When true, estimate token cost instead of returning content.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub estimate: Option<bool>,
    /// Response token budget.
    #[serde(default, deserialize_with = "lenient_u64")]
    pub max_tokens: Option<u64>,
}

pub(crate) use crate::index_lifecycle::guidance::read_contract::{
    file_content_options_from_input, normalize_file_content_aliases,
};

fn add_sections_allowlist(schema: &mut Schema, values: &[&str]) {
    if let Some(serde_json::Value::Object(properties)) = schema.get_mut("properties")
        && let Some(serde_json::Value::Object(sections)) = properties.get_mut("sections")
    {
        sections.insert(
            "items".to_string(),
            serde_json::json!({ "type": "string", "enum": values }),
        );
    }
}

fn add_file_context_schema(schema: &mut Schema) {
    add_include_tests_schema(schema);
    add_sections_allowlist(
        schema,
        &[
            "outline",
            "imports",
            "consumers",
            "references",
            "git",
            "knowledge",
        ],
    );
}

fn add_symbol_context_schema(schema: &mut Schema) {
    add_include_tests_schema(schema);
    add_sections_allowlist(
        schema,
        &[
            "dependents",
            "siblings",
            "implementations",
            "git",
            "knowledge",
        ],
    );
}

#[cfg(test)]
mod knowledge_section_schema_tests {
    use super::*;

    fn section_values<T: JsonSchema>() -> Vec<String> {
        let schema = serde_json::to_value(schemars::schema_for!(T)).expect("schema json");
        schema["properties"]["sections"]["items"]["enum"]
            .as_array()
            .expect("sections enum")
            .iter()
            .map(|value| value.as_str().expect("string enum").to_string())
            .collect()
    }

    #[test]
    fn context_schemas_advertise_the_exact_knowledge_section_allowlists() {
        assert_eq!(
            section_values::<GetFileContextInput>(),
            [
                "outline",
                "imports",
                "consumers",
                "references",
                "git",
                "knowledge",
            ]
        );
        assert_eq!(
            section_values::<GetSymbolContextInput>(),
            [
                "dependents",
                "siblings",
                "implementations",
                "git",
                "knowledge",
            ]
        );
    }
}
