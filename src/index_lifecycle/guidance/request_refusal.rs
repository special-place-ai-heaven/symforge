//! The request check's wording, shared by every lane an LLM talks to (feature 036,
//! contracts/tool-outcome.md): the MCP server (`protocol::tool_contract`, the daemon)
//! and the embed host (`HostRoom::dispatch_wire`) refuse an argument a tool does not
//! define with the same bytes, because they are built here and nowhere else.
//!
//! Also here: the strict decode that finds those arguments (`decode_strict`, keys no
//! field consumed, located in the call), the per-tool valid-argument table the embed
//! lane prints (`TOOL_ARGUMENTS`, pinned to the MCP lane's published schemas by a server
//! test), and which embed request variant answers for which MCP tool
//! (`EMBED_REQUEST_TOOLS`). No refusal echoes an argument VALUE; keys only.

use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

/// The keys of a call that no argument struct of the tool consumed.
#[derive(Debug)]
pub(crate) struct UnknownArguments(pub Vec<String>);

impl std::fmt::Display for UnknownArguments {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown arguments: {}", self.0.join(", "))
    }
}

impl std::error::Error for UnknownArguments {}

/// Why [`decode_strict`] did not return a value.
#[derive(Debug)]
pub(crate) enum StrictError {
    /// Keys that serde did not consume (aliases, nested objects and `flatten` are
    /// honoured by construction), or an `unknown field` error of a
    /// `deny_unknown_fields` struct, located in the call where it can be.
    Unknown(UnknownArguments),
    /// Any other decode failure. The handler reports it in its own words.
    Decode(serde_json::Error),
}

/// `element 2: unknown field `x`, expected ...` -> (Some(2), "x"); without the element
/// prefix -> (None, "x").
fn unknown_field_in(message: &str) -> Option<(Option<usize>, String)> {
    let (before, rest) = message.split_once("unknown field `")?;
    let key = rest.split('`').next()?.to_string();
    let index = before
        .rsplit_once("element ")
        .and_then(|(_, tail)| tail.split(':').next())
        .and_then(|n| n.trim().parse().ok());
    Some((index, key))
}

/// The field names a `deny_unknown_fields` decode error lists as expected, in declaration
/// order: `unknown field `x`, expected one of `a`, `b`` -> [a, b]; `there are no fields`
/// -> []. `None` when the error is not an unknown-field error.
#[cfg(any(feature = "embed", test))]
pub(crate) fn expected_fields_in(message: &str) -> Option<Vec<String>> {
    let (_, rest) = message.split_once("unknown field `")?;
    let (_, listed) = rest.split_once("`, ")?;
    if listed.starts_with("there are no fields") {
        return Some(Vec::new());
    }
    let listed = listed.strip_prefix("expected ")?;
    let listed = listed
        .strip_prefix("one of ")
        .unwrap_or(listed)
        .replace("` or `", "`, `");
    Some(
        listed
            .split("`, `")
            .map(|name| name.trim_matches(|c| c == '`').to_string())
            .map(|name| name.split('`').next().unwrap_or_default().to_string())
            .collect(),
    )
}

thread_local! {
    /// Ignored keys of array elements that a lenient container decoded on its own
    /// (`lenient_vec_required`), where `serde_ignored` cannot see them. `Some` only while
    /// `decode_strict` runs on this thread; the decode is synchronous.
    static NESTED_IGNORED: std::cell::RefCell<Option<Vec<String>>> =
        const { std::cell::RefCell::new(None) };
    /// The array element a lenient container is decoding, for `note_value`.
    static ELEMENT_INDEX: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Switches the thread's nested-key sink on for one `decode_strict` call.
struct NestedSink;

impl NestedSink {
    fn install() -> Self {
        NESTED_IGNORED.with(|sink| *sink.borrow_mut() = Some(Vec::new()));
        Self
    }

    fn take(&self) -> Vec<String> {
        NESTED_IGNORED
            .with(|sink| sink.borrow_mut().take())
            .unwrap_or_default()
    }
}

impl Drop for NestedSink {
    fn drop(&mut self) {
        NESTED_IGNORED.with(|sink| *sink.borrow_mut() = None);
    }
}

/// `serde_json::from_value::<T>` for element `index` of an array that a lenient container
/// decodes itself. While a strict decode is running, keys `T` does not consume are
/// recorded as `[index].key` so the strict decode can refuse them; otherwise it is
/// exactly `serde_json::from_value`.
pub(crate) fn from_value_noting<T: DeserializeOwned>(
    value: Value,
    index: usize,
) -> Result<T, serde_json::Error> {
    let previous = ELEMENT_INDEX.replace(index);
    let decoded = note_value(value);
    ELEMENT_INDEX.set(previous);
    decoded
}

/// [`from_value_noting`] for a value decoded inside an element (the stringified-object
/// path of `SingleEdit`): recorded under the element being decoded.
pub(crate) fn note_value<T: DeserializeOwned>(value: Value) -> Result<T, serde_json::Error> {
    if NESTED_IGNORED.with(|sink| sink.borrow().is_none()) {
        return serde_json::from_value(value);
    }
    let index = ELEMENT_INDEX.get();
    let mut ignored = Vec::new();
    let decoded = serde_ignored::deserialize::<_, _, T>(value, |path| {
        ignored.push(format!("[{index}].{path}"));
    });
    NESTED_IGNORED.with(|sink| {
        if let Some(sink) = sink.borrow_mut().as_mut() {
            sink.extend(ignored);
        }
    });
    decoded
}

/// Record the keys of `value` that `T` does not consume, when a strict decode is running;
/// otherwise nothing. For lenient extraction paths that read `value` by hand.
pub(crate) fn note_keys_of<T: DeserializeOwned>(value: &Value) {
    if NESTED_IGNORED.with(|sink| sink.borrow().is_some()) {
        let _ = note_value::<T>(value.clone());
    }
}

/// The elements of an array argument, whether it arrived as an array or stringified.
fn array_items(value: &Value) -> Option<Vec<Value>> {
    match value {
        Value::Array(items) => Some(items.clone()),
        Value::String(text) => serde_json::from_str::<Vec<Value>>(text).ok(),
        _ => None,
    }
}

/// An array element that is an object, or a string holding one.
fn element_object(value: &Value) -> Option<Map<String, Value>> {
    match value {
        Value::Object(map) => Some(map.clone()),
        Value::String(text) => match serde_json::from_str::<Value>(text).ok()? {
            Value::Object(map) => Some(map),
            _ => None,
        },
        _ => None,
    }
}

/// Dotted path of the first object member named `key`, searching nested objects.
fn find_key_path(map: &Map<String, Value>, key: &str) -> Option<String> {
    if map.contains_key(key) {
        return Some(key.to_string());
    }
    map.iter().find_map(|(name, value)| match value {
        Value::Object(inner) => find_key_path(inner, key).map(|rest| format!("{name}.{rest}")),
        _ => None,
    })
}

/// Locate `key` (relative to an array element, at `index` when known) in the call:
/// `edits.1.operation.zz`. When no top-level array holds it, the key as given, or
/// `[index].key` for a known element.
fn qualify(index: Option<usize>, key: &str, root: &Value) -> String {
    let first = key.split('.').next().unwrap_or(key);
    let found = root.as_object().and_then(|object| {
        object.iter().find_map(|(field, value)| {
            let items = array_items(value)?;
            let range = match index {
                Some(i) => i..i.saturating_add(1),
                None => 0..items.len(),
            };
            range.into_iter().find_map(|i| {
                let element = element_object(items.get(i)?)?;
                let path = if element.contains_key(first) {
                    key.to_string()
                } else {
                    find_key_path(&element, key)?
                };
                Some(format!("{field}.{i}.{path}"))
            })
        })
    });
    match (found, index) {
        (Some(path), _) => path,
        (None, Some(i)) => format!("[{i}].{key}"),
        (None, None) => key.to_string(),
    }
}

/// Decode `value` as `P` and report every key that no field of `P` consumed (FR-001).
///
/// Leniency about VALUE types lives in `P`'s own deserializers and is untouched; only
/// keys are judged. Paths are dotted (`edits.0.zz`); the first segment is a top-level
/// argument.
pub(crate) fn decode_strict<P: DeserializeOwned>(value: Value) -> Result<P, StrictError> {
    let mut ignored = Vec::new();
    let sink = NestedSink::install();
    let decoded = serde_ignored::deserialize::<_, _, P>(&value, |path| {
        ignored.push(path.to_string());
    });
    ignored.extend(sink.take().iter().map(|entry| {
        match entry
            .strip_prefix('[')
            .and_then(|rest| rest.split_once("]."))
            .and_then(|(index, key)| Some((index.parse::<usize>().ok()?, key)))
        {
            Some((index, key)) => qualify(Some(index), key, &value),
            None => entry.clone(),
        }
    }));
    if !ignored.is_empty() {
        return Err(StrictError::Unknown(UnknownArguments(ignored)));
    }
    decoded.map_err(|error| match unknown_field_in(&error.to_string()) {
        Some((index, key)) => {
            let located = if index.is_none() && value.get(&key).is_some() {
                key
            } else {
                qualify(index, &key, &value)
            };
            StrictError::Unknown(UnknownArguments(vec![located]))
        }
        None => StrictError::Decode(error),
    })
}

/// An echoed argument NAME is cut to this many characters.
const KEY_ECHO_LIMIT: usize = 40;

fn capped_key(key: &str) -> String {
    if key.chars().count() <= KEY_ECHO_LIMIT {
        return key.to_string();
    }
    let cut: String = key.chars().take(KEY_ECHO_LIMIT).collect();
    format!("{cut}...")
}

/// Refusal text for keys no tool argument consumed (contracts/tool-outcome.md).
pub(crate) fn unknown_arguments_refusal(
    tool: &str,
    unknown: &[String],
    valid: &[String],
) -> String {
    let named = unknown
        .iter()
        .map(|key| format!("\"{}\"", capped_key(key)))
        .collect::<Vec<_>>()
        .join(", ");
    let valid = if valid.is_empty() {
        "none".to_string()
    } else {
        valid.join(", ")
    };
    let mut text =
        format!("invalid_request: {tool} does not accept {named}. Valid arguments: {valid}.");
    if let Some(parent) = unknown
        .iter()
        .find_map(|k| k.split_once('.').map(|(p, _)| p))
    {
        text.push_str(&format!(
            " Check the fields of \"{parent}\" against the tool's input schema."
        ));
    }
    text
}

// ── mode-condition refusals (FR-002) ───────────────────────────────────────────────

/// Stand-in value in [`When::Equals`] for "the controlling argument is absent".
pub(crate) const ABSENT: &str = "(absent)";

/// The mode in which an argument applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum When {
    /// The controlling argument is one of `values` (use [`ABSENT`] for "not given").
    #[allow(dead_code)]
    // ponytail: no table row after the handler audit; synthetic rows in tests
    Equals(&'static str, &'static [&'static str]),
    /// The controlling argument is NOT one of `values`: the argument is ignored only when
    /// the controller equals one of them, and every other value reaches a branch that
    /// reads it (a handler's default branch).
    Unless(&'static str, &'static [&'static str]),
    /// The controlling argument is given.
    #[allow(dead_code)] // ponytail: no table row; synthetic rows in tests
    Present(&'static str),
    /// The controlling argument is not given.
    Absent(&'static str),
}

/// A mode the server chose because a lone argument uses only that mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ModeSelection {
    pub controller: &'static str,
    pub mode: &'static str,
    /// The argument whose presence selected the mode.
    pub argument: &'static str,
}

/// A raw argument as the handlers' lenient decoders read it: null, `""` and `[]` are
/// absent (FR-003), `"true"`/`"1"` and `"false"`/`"0"` are booleans. `None` is absent.
pub(crate) fn normalized(value: Option<&Value>) -> Option<Value> {
    match value? {
        Value::Null => None,
        Value::String(s) => match s.as_str() {
            "" => None,
            "true" | "1" => Some(Value::Bool(true)),
            "false" | "0" => Some(Value::Bool(false)),
            _ => Some(Value::String(s.clone())),
        },
        Value::Array(items) if items.is_empty() => None,
        other => Some(other.clone()),
    }
}

/// The controlling argument's value as a condition compares it.
pub(crate) fn current_value(args: &Map<String, Value>, controller: &str) -> String {
    match normalized(args.get(controller)) {
        None => ABSENT.to_string(),
        Some(Value::String(s)) => s,
        Some(other) => other.to_string(),
    }
}

pub(crate) fn literal(value: &str) -> String {
    if matches!(value, "true" | "false") {
        value.to_string()
    } else {
        format!("\"{value}\"")
    }
}

/// Refusal text for `argument` given outside the mode `applies_when` names.
pub(crate) fn condition_refusal(
    tool: &str,
    argument: &str,
    applies_when: &When,
    args: &Map<String, Value>,
    selected: &[ModeSelection],
) -> String {
    // No refusal echoes a value the caller sent (a path may be sensitive and a later guard
    // of the handler refuses it without echo): it names arguments, and the controller's
    // expected values from the table.
    let listed = |values: &[&str]| {
        let modes: Vec<String> = values
            .iter()
            .map(|v| {
                if *v == ABSENT {
                    "omitted".to_string()
                } else {
                    format!("\"{v}\"")
                }
            })
            .collect();
        match modes.split_last() {
            Some((last, rest)) if !rest.is_empty() => format!("{} or {last}", rest.join(", ")),
            _ => modes.join(""),
        }
    };
    match applies_when {
        When::Equals(controller, values) => {
            let modes = listed(values);
            let mode = values
                .iter()
                .find(|v| **v != ABSENT)
                .map_or_else(String::new, |v| literal(v));
            if let Some(chosen) = selected.iter().find(|s| s.controller == *controller) {
                return format!(
                    "invalid_request: \"{argument}\" applies when {controller} is {modes}; \
                     `{}` selected {controller}={} for this call. Drop \"{argument}\" or drop \
                     `{}`.",
                    chosen.argument,
                    literal(chosen.mode),
                    chosen.argument
                );
            }
            let used = if current_value(args, controller) == ABSENT {
                format!("did not set {controller}")
            } else {
                format!("used a different {controller}")
            };
            format!(
                "invalid_request: \"{argument}\" applies when {controller} is {modes}; this call \
                 {used}. Call {tool}({controller}={mode}, {argument}=...) or drop \"{argument}\"."
            )
        }
        When::Unless(controller, values) => format!(
            "invalid_request: \"{argument}\" does not apply when {controller} is {}; this call \
             set {controller} to {}. Drop \"{argument}\" or use another {controller}.",
            listed(values),
            if values.len() == 1 {
                "it"
            } else {
                "one of them"
            }
        ),
        When::Absent(controller) => format!(
            "invalid_request: \"{argument}\" applies only when \"{controller}\" is not given; \
             this call gave \"{controller}\". Drop \"{argument}\" or drop \"{controller}\"."
        ),
        When::Present(controller) => format!(
            "invalid_request: \"{argument}\" applies only with \"{controller}\"; this call did \
             not give it. Add \"{controller}\" or drop \"{argument}\"."
        ),
    }
}

/// Refusal text for two arguments that select different modes of one controller.
pub(crate) fn conflicting_selection_refusal(
    controller: &str,
    earlier: &ModeSelection,
    argument: &str,
    mode: &str,
) -> String {
    format!(
        "invalid_request: \"{}\" selects {controller}={} and \"{argument}\" selects \
         {controller}={}; they cannot be combined. Drop one of them.",
        earlier.argument,
        literal(earlier.mode),
        literal(mode)
    )
}

// ── the embed lane's view of the MCP tools ─────────────────────────────────────────

/// Every registered MCP tool and its valid top-level arguments, in the order the MCP lane
/// publishes and prints them (the properties of its input schema). Pinned to the live
/// schemas by `tool_arguments_match_the_published_schemas` (server builds).
#[cfg(any(feature = "embed", test))]
pub(crate) static TOOL_ARGUMENTS: &[(&str, &[&str])] = &[
    (
        "analyze_file_impact",
        &[
            "co_changes_limit",
            "estimate",
            "include_co_changes",
            "new_file",
            "path",
            "project",
        ],
    ),
    ("ask", &["max_tokens", "project", "query"]),
    (
        "batch_edit",
        &[
            "dry_run",
            "edits",
            "idempotency_key",
            "project",
            "working_directory",
        ],
    ),
    (
        "batch_insert",
        &[
            "content",
            "dry_run",
            "idempotency_key",
            "position",
            "project",
            "targets",
            "working_directory",
        ],
    ),
    (
        "batch_rename",
        &[
            "code_only",
            "dry_run",
            "idempotency_key",
            "kind",
            "name",
            "new_name",
            "path",
            "project",
            "symbol_line",
            "working_directory",
        ],
    ),
    ("checkpoint_now", &["export_artifact", "verify_after_write"]),
    ("context_inventory", &[]),
    ("conventions", &[]),
    (
        "curate_knowledge",
        &[
            "actions",
            "apply",
            "idempotency_key",
            "if_manifest_digest",
            "if_policy_digest",
            "if_source_review_hash",
            "project",
        ],
    ),
    (
        "delete_symbol",
        &[
            "dry_run",
            "idempotency_key",
            "kind",
            "name",
            "path",
            "project",
            "symbol_line",
            "working_directory",
        ],
    ),
    (
        "detect_impact",
        &[
            "base_branch",
            "depth",
            "include_data",
            "include_untracked",
            "scope",
            "since",
        ],
    ),
    (
        "diff_symbols",
        &[
            "base",
            "code_only",
            "compact",
            "estimate",
            "language",
            "max_tokens",
            "path_prefix",
            "project",
            "summary_only",
            "target",
        ],
    ),
    ("edit_plan", &["project", "target"]),
    (
        "edit_within_symbol",
        &[
            "dry_run",
            "idempotency_key",
            "kind",
            "name",
            "near_line",
            "new_text",
            "occurrence",
            "old_text",
            "path",
            "project",
            "replace_all",
            "symbol_line",
            "working_directory",
        ],
    ),
    (
        "explore",
        &[
            "depth",
            "estimate",
            "include_noise",
            "include_personal_tooling",
            "include_vendor",
            "language",
            "limit",
            "max_tokens",
            "path_prefix",
            "project",
            "query",
        ],
    ),
    (
        "find_dependents",
        &[
            "compact",
            "estimate",
            "format",
            "limit",
            "max_per_file",
            "max_tokens",
            "name",
            "path",
            "project",
        ],
    ),
    (
        "find_references",
        &[
            "compact",
            "direction",
            "estimate",
            "kind",
            "limit",
            "max_per_file",
            "max_tokens",
            "mode",
            "name",
            "path",
            "project",
            "projects",
            "symbol_kind",
            "symbol_line",
        ],
    ),
    (
        "get_file_content",
        &[
            "around_line",
            "around_match",
            "around_symbol",
            "chunk_index",
            "context_lines",
            "end_line",
            "estimate",
            "force_refresh",
            "header",
            "limit",
            "match_occurrence",
            "max_lines",
            "max_tokens",
            "mode",
            "offset",
            "path",
            "project",
            "show_line_numbers",
            "start_line",
            "symbol_line",
        ],
    ),
    (
        "get_file_context",
        &[
            "estimate",
            "force_refresh",
            "include_tests",
            "max_tokens",
            "path",
            "project",
            "sections",
        ],
    ),
    (
        "get_repo_map",
        &[
            "depth",
            "detail",
            "estimate",
            "include_knowledge",
            "max_files",
            "max_tokens",
            "path",
            "project",
        ],
    ),
    (
        "get_symbol",
        &[
            "estimate",
            "force_refresh",
            "kind",
            "line_numbers",
            "max_tokens",
            "name",
            "path",
            "project",
            "symbol_line",
            "targets",
        ],
    ),
    (
        "get_symbol_context",
        &[
            "bundle",
            "estimate",
            "file",
            "include_tests",
            "max_tokens",
            "name",
            "path",
            "project",
            "sections",
            "symbol_kind",
            "symbol_line",
            "verbosity",
        ],
    ),
    ("health", &["quarantine_limit", "quarantine_offset"]),
    ("health_compact", &[]),
    (
        "index_folder",
        &["add", "allow_protected_root", "idempotency_key", "path"],
    ),
    (
        "insert_symbol",
        &[
            "content",
            "dry_run",
            "idempotency_key",
            "kind",
            "name",
            "path",
            "position",
            "project",
            "symbol_line",
            "working_directory",
        ],
    ),
    (
        "inspect_match",
        &[
            "context",
            "estimate",
            "line",
            "max_tokens",
            "path",
            "sibling_limit",
        ],
    ),
    ("investigation_suggest", &["focus", "project"]),
    (
        "replace_symbol_body",
        &[
            "dry_run",
            "idempotency_key",
            "if_match",
            "kind",
            "name",
            "new_body",
            "path",
            "project",
            "symbol_line",
            "working_directory",
        ],
    ),
    (
        "review_knowledge",
        &[
            "limit",
            "max_tokens",
            "mode",
            "path",
            "path_prefix",
            "project",
            "projects",
            "source_scope",
        ],
    ),
    (
        "search_files",
        &[
            "anchor_path",
            "changed_with",
            "current_file",
            "debug_ranking",
            "estimate",
            "include_personal_tooling",
            "include_vendor",
            "limit",
            "max_tokens",
            "path_prefix",
            "project",
            "projects",
            "query",
            "rank_by",
            "resolve",
        ],
    ),
    (
        "search_knowledge",
        &[
            "authority_scope",
            "include_archived",
            "limit",
            "max_tokens",
            "path_prefix",
            "project",
            "projects",
            "query",
            "source_scope",
        ],
    ),
    (
        "search_symbols",
        &[
            "estimate",
            "include_generated",
            "include_personal_tooling",
            "include_tests",
            "include_vendor",
            "kind",
            "language",
            "limit",
            "max_tokens",
            "path_prefix",
            "project",
            "projects",
            "query",
        ],
    ),
    (
        "search_text",
        &[
            "case_sensitive",
            "context",
            "estimate",
            "exclude_glob",
            "follow_refs",
            "follow_refs_limit",
            "glob",
            "group_by",
            "include_body",
            "include_generated",
            "include_personal_tooling",
            "include_tests",
            "include_vendor",
            "language",
            "limit",
            "max_per_file",
            "max_tokens",
            "path_prefix",
            "project",
            "projects",
            "query",
            "ranked",
            "regex",
            "structural",
            "terms",
            "whole_word",
        ],
    ),
    (
        "secret_remediate",
        &[
            "action",
            "finding_ids",
            "idempotency_key",
            "preview",
            "scope",
        ],
    ),
    ("status", &["detail", "reset_calibration"]),
    (
        "symforge",
        &[
            "intent",
            "max_tokens",
            "path",
            "preview",
            "project",
            "projects",
            "query",
            "symbol",
        ],
    ),
    (
        "symforge_edit",
        &[
            "apply",
            "body",
            "idempotency_key",
            "if_match",
            "intent",
            "new_text",
            "old_text",
            "op",
            "path",
            "replace_all",
            "symbol",
        ],
    ),
    ("symforge_retrieve", &["hash"]),
    ("validate_file_syntax", &["path", "project"]),
    (
        "what_changed",
        &[
            "code_only",
            "estimate",
            "git_ref",
            "include_symbol_diff",
            "language",
            "max_tokens",
            "path_prefix",
            "project",
            "since",
            "uncommitted",
        ],
    ),
];

/// The valid top-level arguments of MCP tool `tool`.
#[cfg(any(feature = "embed", test))]
pub(crate) fn tool_arguments(tool: &str) -> Option<&'static [&'static str]> {
    TOOL_ARGUMENTS
        .iter()
        .find(|(name, _)| *name == tool)
        .map(|(_, arguments)| *arguments)
}

/// Which MCP tool each embed host request answers for, by its wire label
/// (`Query.<QueryRequest variant>`, `Edit.<WireEditRequest kind>`,
/// `Knowledge.<WireKnowledgeRequest kind>`, or the `HostRequest` variant). `None`: no
/// MCP tool takes this request; its refusal names the request label instead.
#[cfg(any(feature = "embed", test))]
pub(crate) static EMBED_REQUEST_TOOLS: &[(&str, Option<&str>)] = &[
    ("Status", None),
    ("StatusReport", Some("status")),
    ("Health", None),
    ("HealthReport", Some("health")),
    ("HealthCompact", Some("health_compact")),
    ("Catalog", None),
    ("Resource", None),
    ("Prompt", None),
    ("SymforgeEdit", Some("symforge_edit")),
    ("InspectSecrets", None), // implements secret_remediate over typed findings
    ("PreviewSecretRemediation", None), // implements secret_remediate over typed findings
    ("ApplySecretRemediation", None), // implements secret_remediate over typed findings
    ("Refresh", None),
    ("RefreshWith", None), // implements index_folder for the bound source only
    ("Checkpoint", Some("checkpoint_now")),
    ("Query.TextSearch", Some("search_text")),
    ("Query.SymbolSearch", Some("search_symbols")),
    ("Query.FileSearch", Some("search_files")),
    ("Query.SearchKnowledge", Some("search_knowledge")),
    ("Query.FileContent", Some("get_file_content")),
    ("Query.RepoMap", Some("get_repo_map")),
    ("Query.FileContext", Some("get_file_context")),
    ("Query.SourcePage", None),
    ("Query.SymbolRead", Some("get_symbol")),
    ("Query.SymbolContext", Some("get_symbol_context")),
    ("Query.InspectMatch", Some("inspect_match")),
    ("Query.ReferenceSearch", Some("find_references")),
    ("Query.DependentSearch", Some("find_dependents")),
    // The V11 request shapes: kept for source compatibility, no MCP tool takes them.
    ("Query.File", None),
    ("Query.Symbol", None),
    ("Query.Context", None),
    ("Query.References", None),
    ("Query.Dependents", None),
    ("Query.SearchSymbols", None),
    ("Query.SearchText", None),
    ("Query.SearchFiles", None),
    ("Query.Graph", None),
    ("Query.Diff", None),
    ("Query.Impact", None),
    ("Query.Syntax", Some("validate_file_syntax")),
    ("Query.FileImpact", Some("analyze_file_impact")),
    ("Query.Explore", Some("explore")),
    ("Query.Conventions", Some("conventions")),
    ("Query.ContextInventory", Some("context_inventory")),
    ("Query.InvestigationSuggest", Some("investigation_suggest")),
    ("Query.EditPlan", Some("edit_plan")),
    ("Query.Retrieve", None), // implements symforge_retrieve by handle and offset
    ("Query.WhatChanged", Some("what_changed")),
    ("Query.DiffSymbols", Some("diff_symbols")),
    ("Query.DetectImpact", Some("detect_impact")),
    ("Query.Ask", Some("ask")),
    ("Query.Symforge", Some("symforge")),
    ("Edit.plan", Some("edit_plan")),
    ("Edit.plan_batch_rename", None), // implements batch_rename over source guards
    ("Edit.preview_replace", None),   // implements replace_symbol_body over source guards
    ("Edit.apply_replace", None),     // implements replace_symbol_body over source guards
    ("Edit.preview_insert", None),    // implements insert_symbol over source guards
    ("Edit.apply_insert", None),      // implements insert_symbol over source guards
    ("Edit.preview_delete", None),    // implements delete_symbol over source guards
    ("Edit.apply_delete", None),      // implements delete_symbol over source guards
    ("Edit.preview_edit_within", None), // implements edit_within_symbol over source guards
    ("Edit.apply_edit_within", None), // implements edit_within_symbol over source guards
    ("Edit.preview_batch_edit", None), // implements batch_edit over source guards
    ("Edit.apply_batch_edit", None),  // implements batch_edit over source guards
    ("Edit.preview_batch_insert", None), // implements batch_insert over source guards
    ("Edit.apply_batch_insert", None), // implements batch_insert over source guards
    ("Edit.preview_batch_rename", None), // implements batch_rename over source guards
    ("Edit.apply_batch_rename", None), // implements batch_rename over source guards
    ("Knowledge.review", Some("review_knowledge")),
    ("Knowledge.curate_preview", Some("curate_knowledge")),
    ("Knowledge.curate_apply", Some("curate_knowledge")),
];

/// The refusal an embed request with `unknown` keys gets. `label` is its wire label
/// ([`EMBED_REQUEST_TOOLS`]); `embed_fields` the fields its request type declares, when
/// they can be read (`None` for a type that accepts unknown keys, whose fields are the
/// MCP tool's own). When the request answers for an MCP tool, a key that tool does not
/// accept either gets the MCP lane's text, byte for byte. A key the MCP tool accepts but
/// this request does not (`project` on most query requests) gets the text under the MCP
/// tool's name listing this request's own fields, since the MCP list names that key.
/// A request answering for no MCP tool is named by its label, with its own fields.
#[cfg(feature = "embed")]
pub(crate) fn embed_unknown_arguments_refusal(
    label: &str,
    unknown: &[String],
    embed_fields: Option<&[String]>,
) -> String {
    let tool = EMBED_REQUEST_TOOLS
        .iter()
        .find(|(name, _)| *name == label)
        .and_then(|(_, tool)| *tool);
    let published = tool.and_then(tool_arguments);
    let owned = |names: &[&str]| names.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
    let valid: Vec<String> = match (published, embed_fields) {
        // A key the MCP tool accepts but this request does not: name what this one takes.
        (Some(published), Some(fields))
            if unknown.iter().any(|key| published.contains(&key.as_str())) =>
        {
            fields.to_vec()
        }
        (Some(published), _) => owned(published),
        (None, fields) => fields.map(<[String]>::to_vec).unwrap_or_default(),
    };
    unknown_arguments_refusal(tool.unwrap_or(label), unknown, &valid)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Echoed values are cut; the cut keeps the start.
    #[test]
    fn echoed_names_are_cut() {
        let long = "x".repeat(500);
        assert!(capped_key(&long).chars().count() < KEY_ECHO_LIMIT + 8);
        assert_eq!(capped_key("zz"), "zz");
        let text = unknown_arguments_refusal("t", std::slice::from_ref(&long), &["a".to_string()]);
        assert!(!text.contains(&"x".repeat(60)), "{text}");
    }

    #[test]
    fn expected_fields_are_read_from_serde_messages() {
        assert_eq!(
            expected_fields_in("unknown field `x`, expected one of `a`, `b`, `c`"),
            Some(vec!["a".to_string(), "b".to_string(), "c".to_string()])
        );
        assert_eq!(
            expected_fields_in("unknown field `x`, expected `a` or `b`"),
            Some(vec!["a".to_string(), "b".to_string()])
        );
        assert_eq!(
            expected_fields_in("unknown field `x`, expected `a`"),
            Some(vec!["a".to_string()])
        );
        assert_eq!(
            expected_fields_in("unknown field `x`, there are no fields"),
            Some(Vec::new())
        );
        assert_eq!(expected_fields_in("missing field `a`"), None);
    }

    #[test]
    fn every_embed_request_tool_is_a_published_tool() {
        for (label, tool) in EMBED_REQUEST_TOOLS {
            if let Some(tool) = tool {
                assert!(
                    tool_arguments(tool).is_some(),
                    "{label} names `{tool}`, which TOOL_ARGUMENTS does not list"
                );
            }
        }
    }

    /// The table the embed lane prints equals what the MCP lane publishes today, tool by
    /// tool and in order, so the two lanes cannot drift apart.
    #[cfg(feature = "server")]
    #[test]
    fn tool_arguments_match_the_published_schemas() {
        let published: Vec<(String, Vec<String>)> =
            crate::protocol::SymForgeServer::tool_definitions()
                .into_iter()
                .map(|tool| {
                    let arguments = tool
                        .input_schema
                        .get("properties")
                        .and_then(Value::as_object)
                        .map(|p| p.keys().cloned().collect())
                        .unwrap_or_default();
                    (tool.name.to_string(), arguments)
                })
                .collect();
        let mut published = published;
        published.sort();
        let mut table: Vec<(String, Vec<String>)> = TOOL_ARGUMENTS
            .iter()
            .map(|(tool, arguments)| {
                (
                    (*tool).to_string(),
                    arguments.iter().map(|a| (*a).to_string()).collect(),
                )
            })
            .collect();
        table.sort();
        let literal = published
            .iter()
            .map(|(tool, arguments)| {
                let quoted: Vec<String> = arguments.iter().map(|a| format!("\"{a}\"")).collect();
                format!("    (\"{tool}\", &[{}]),", quoted.join(", "))
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            table, published,
            "TOOL_ARGUMENTS drifted from the published schemas; the table is:\n{literal}"
        );
    }
}
