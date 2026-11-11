//! Per-tool contract table (feature 036, FR-004; data-model.md `ToolContract`).
//!
//! One static row per registered tool: the conditions under which a mode-specific
//! argument applies, the content the tool exists to return, its optional sections in
//! the order they are dropped under pressure, the arguments whose presence marks
//! content as explicitly requested, and the argument set of its typical call.
//!
//! GOVERNING RULE for `conditions`: a row may refuse ONLY where the handler provably
//! ignores the argument in that situation, and each row cites the handler line that does
//! (audit of 2026-11-11). If a handler reads the argument at all in a mode, the row must
//! allow that mode; when unsure there is no row. A missed refusal is acceptable, a false
//! refusal is not.
//!
//! The set of valid argument names is NOT stored here; it is the tool's published input
//! schema. The tests below check that every name used in this table is in that schema.
//!
//! Written from the handlers on 2026-11-11. Where a handler's doc comment and its code
//! disagree (`get_repo_map` `path` and `max_files`, research D2) the row follows the
//! code and the doc is brought in line; a test requires every conditioned argument's
//! published description to name the argument that controls it.
//!
//! The request check also lives here: `decode_strict` (keys no argument struct consumes)
//! and `apply_conditions` (mode rules), run by `SymForgeServer::refuse_invalid_arguments`
//! at the top of `call_tool` and, for unknown keys, by the daemon's `decode_params`.
// ponytail: `optional_sections` is read by the sectioned budget (`in_drop_order`) and
// `conditions` by the request check (`apply_conditions`); `primary` and
// `explicit_arguments` are read by T050. The field-level allows below go then.

use rmcp::model::{CallToolResult, JsonObject};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::protocol::SymForgeServer;
use crate::protocol::edit::{
    BatchEditInput, BatchInsertInput, BatchRenameInput, DeleteSymbolInput, EditWithinSymbolInput,
    InsertSymbolInput, ReplaceSymbolBodyInput,
};
use crate::protocol::read_tools::SymforgeRetrieveInput;
use crate::protocol::result_status::{OutcomeClass, ResultStatus};
use crate::protocol::secret_remediate::SecretRemediateInput;
use crate::protocol::tools::{
    AnalyzeFileImpactInput, CheckpointNowInput, CurateKnowledgeInput, DetectImpactInput,
    DiffSymbolsInput, EditPlanInput, ExploreInput, FindDependentsInput, FindReferencesInput,
    GetFileContentInput, GetFileContextInput, GetRepoMapInput, GetSymbolContextInput,
    GetSymbolInput, HealthInput, IndexFolderInput, InspectMatchInput, InvestigationInput,
    ReviewKnowledgeInput, SearchFilesInput, SearchKnowledgeInput, SearchSymbolsInput,
    SearchTextInput, SmartQueryInput, ValidateFileSyntaxInput, WhatChangedInput,
};
use crate::stel::{StelEditRequest, StelRequest, StelStatusRequest};

/// Stand-in value in [`When::Equals`] for "the controlling argument is absent".
pub(crate) const ABSENT: &str = "(absent)";

/// What a condition does when the argument is given outside the mode that uses it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OnMismatch {
    /// Exactly one mode uses the argument and the caller chose none: select that mode.
    #[allow(dead_code)]
    // ponytail: no table row after the handler audit; synthetic rows in tests
    SelectMode,
    /// Refuse, naming the mode that accepts the argument.
    Refuse,
}

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

/// `argument` applies only when `applies_when` holds.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Condition {
    pub argument: &'static str,
    pub applies_when: When,
    pub on_mismatch: OnMismatch,
    /// When the controlling argument is absent and this one is given, the handler picks
    /// the mode itself and uses `argument`, so the call is accepted as it is.
    #[allow(dead_code)]
    // ponytail: no table row after the handler audit; synthetic rows in tests
    pub adaptive_with: Option<&'static str>,
}

#[derive(Debug)]
pub(crate) struct ToolContract {
    /// Registered tool name, without the `mcp__symforge__` client prefix.
    pub tool: &'static str,
    /// Mode rules, enforced before the handler runs (`apply_conditions`).
    pub conditions: &'static [Condition],
    /// The content the tool exists to return.
    #[allow(dead_code)] // ponytail: read by T050
    pub primary: &'static str,
    /// Sections dropped under pressure, first entry dropped first. Only rows whose
    /// handlers implement the order list one (`get_file_context`, `get_symbol_context`).
    pub optional_sections: &'static [&'static str],
    /// Arguments whose presence marks content as explicitly requested (FR-024).
    #[allow(dead_code)] // ponytail: read by T050
    pub explicit_arguments: &'static [&'static str],
    /// JSON object of the typical call, on the reliability fixture. A `{name}` value is a
    /// placeholder the payload test (T050) fills from an earlier call.
    pub typical_call: &'static str,
}

const fn cond(argument: &'static str, applies_when: When, on_mismatch: OnMismatch) -> Condition {
    Condition {
        argument,
        applies_when,
        on_mismatch,
        adaptive_with: None,
    }
}

use OnMismatch::{Refuse, SelectMode};
use When::{Absent, Unless};

const fn row(
    tool: &'static str,
    primary: &'static str,
    typical_call: &'static str,
) -> ToolContract {
    ToolContract {
        tool,
        conditions: &[],
        primary,
        optional_sections: &[],
        explicit_arguments: &[],
        typical_call,
    }
}

impl ToolContract {
    const fn when(mut self, conditions: &'static [Condition]) -> Self {
        self.conditions = conditions;
        self
    }
    const fn drops(mut self, optional_sections: &'static [&'static str]) -> Self {
        self.optional_sections = optional_sections;
        self
    }
    const fn explicit(mut self, explicit_arguments: &'static [&'static str]) -> Self {
        self.explicit_arguments = explicit_arguments;
        self
    }
}

/// Every registered tool, once.
pub(crate) static TOOL_CONTRACTS: &[ToolContract] = &[
    // -- lifecycle and health --
    row("health", "index state, counts and watcher state", "{}")
        .explicit(&["quarantine_limit", "quarantine_offset"]),
    row("health_compact", "one-line index and watcher state", "{}"),
    row("checkpoint_now", "snapshot write confirmation", "{}"),
    row(
        "index_folder",
        "index summary of the opened folder",
        r#"{"path":"{root}"}"#,
    ),
    row(
        "validate_file_syntax",
        "syntax verdict with diagnostic locations",
        r#"{"path":"src/lib.rs"}"#,
    ),
    // -- reads --
    row(
        "get_file_content",
        "the requested lines, numbered",
        r#"{"path":"src/long.rs","start_line":1,"end_line":75}"#,
    )
    .explicit(&[
        "start_line",
        "end_line",
        "around_line",
        "around_match",
        "around_symbol",
        "chunk_index",
        "offset",
        "limit",
    ]),
    row(
        "get_symbol",
        "the full source of each named symbol",
        r#"{"path":"src/long.rs","name":"long_body"}"#,
    )
    // Batch mode (a non-empty `targets`) returns at src/protocol/tools.rs:1759 having read
    // only the elements of `targets`: the top-level path, name, kind and symbol_line are
    // not read on that branch (tools.rs:1649-1760).
    .when(&[
        cond("path", Absent("targets"), Refuse),
        cond("name", Absent("targets"), Refuse),
        cond("kind", Absent("targets"), Refuse),
        cond("symbol_line", Absent("targets"), Refuse),
    ])
    .explicit(&["name", "targets"]),
    row(
        "get_repo_map",
        "directory and key-type overview, source directories first",
        "{}",
    )
    // `depth` is read by the tree branch (tools.rs:2130) and by the default branch that
    // every other detail value reaches (tools.rs:2140); the `"full"` branch
    // (tools.rs:2063-2129) never reads it. Only an explicit detail="full" is refused.
    // `path` is read at every level and `max_files` by more than one branch: no rows.
    .when(&[cond("depth", Unless("detail", &["full"]), Refuse)])
    .explicit(&["detail", "path", "depth"]),
    row(
        "get_file_context",
        "outline of the file",
        r#"{"path":"src/parent/leaf.rs"}"#,
    )
    .drops(&["knowledge"])
    .explicit(&["sections"]),
    row(
        "get_symbol_context",
        "definition with every caller and callee named",
        r#"{"name":"target_fn","path":"src/parent/leaf.rs"}"#,
    )
    .drops(&["knowledge"])
    .explicit(&["sections", "bundle"]),
    row(
        "search_symbols",
        "symbols whose name matches the query",
        r#"{"query":"long_body"}"#,
    ),
    row(
        "search_text",
        "matching lines with their file and line",
        r#"{"query":"LONG_FIRST"}"#,
    ),
    row(
        "search_files",
        "paths ranked by relevance to the query",
        r#"{"query":"leaf"}"#,
    ),
    row(
        "search_knowledge",
        "matching documentation passages with provenance",
        r#"{"query":"zebrafish_quorum"}"#,
    ),
    row(
        "review_knowledge",
        "knowledge-corpus review for the chosen mode",
        r#"{"mode":"summary"}"#,
    )
    .explicit(&["path"]),
    row(
        "curate_knowledge",
        "preview or result of the policy actions",
        r#"{"actions":[],"if_source_review_hash":"{source_review_hash}","if_manifest_digest":"{manifest_digest}","if_policy_digest":"{policy_digest}","apply":false}"#,
    ),
    row(
        "find_references",
        "every reference site of the symbol",
        r#"{"name":"target_fn","path":"src/parent/leaf.rs"}"#,
    )
    .explicit(&["name"]),
    row(
        "find_dependents",
        "every file that depends on the file",
        r#"{"path":"src/parent/leaf.rs"}"#,
    )
    .explicit(&["path"]),
    row(
        "inspect_match",
        "enclosing symbol, parent and siblings of the line",
        r#"{"path":"src/parent/leaf.rs","line":8}"#,
    )
    .explicit(&["line"]),
    row(
        "analyze_file_impact",
        "symbol churn and dependents of the re-read file",
        r#"{"path":"src/parent/leaf.rs"}"#,
    ),
    row(
        "what_changed",
        "the changed paths",
        r#"{"uncommitted":true}"#,
    ),
    row(
        "diff_symbols",
        "added, removed and modified symbols between the refs",
        r#"{"base":"HEAD"}"#,
    ),
    row(
        "detect_impact",
        "affected symbols or files ranked by distance",
        r#"{"base_branch":"main"}"#,
    ),
    row(
        "explore",
        "symbols, text patterns and files for the concept",
        r#"{"query":"target function"}"#,
    ),
    // -- edits --
    row(
        "replace_symbol_body",
        "edit confirmation with the changed range",
        r#"{"path":"src/short.rs","name":"SHORT_A","new_body":"pub const SHORT_A: u32 = 1;","dry_run":true}"#,
    ),
    row(
        "edit_within_symbol",
        "edit confirmation with the changed range",
        r#"{"path":"src/user.rs","name":"use_it","old_text":"target_fn(7)","new_text":"target_fn(8)","dry_run":true}"#,
    ),
    row(
        "insert_symbol",
        "insert confirmation with the new range",
        r#"{"path":"src/short.rs","name":"SHORT_A","content":"pub const SHORT_Z: u32 = 26;","dry_run":true}"#,
    ),
    row(
        "delete_symbol",
        "delete confirmation with the removed range",
        r#"{"path":"src/short.rs","name":"SHORT_G","dry_run":true}"#,
    ),
    row(
        "batch_edit",
        "per-edit results of the atomic batch",
        r#"{"edits":[{"path":"src/short.rs","name":"SHORT_A","operation":{"type":"replace","new_body":"pub const SHORT_A: u32 = 1;"}}],"dry_run":true}"#,
    ),
    row(
        "batch_insert",
        "per-target results of the insertion",
        r#"{"content":"// x","position":"before","targets":[{"path":"src/short.rs","name":"SHORT_A"}],"dry_run":true}"#,
    ),
    row(
        "batch_rename",
        "every site the rename changes and every site it leaves alone",
        r#"{"path":"src/parent/leaf.rs","name":"target_fn","new_name":"renamed_fn","dry_run":true}"#,
    ),
    row(
        "edit_plan",
        "recommended edit tool with reference count",
        r#"{"target":"target_fn"}"#,
    ),
    // -- guidance and session --
    row(
        "ask",
        "the routed tool's answer",
        r#"{"query":"who calls target_fn"}"#,
    ),
    row(
        "conventions",
        "detected conventions of the repository",
        "{}",
    ),
    row(
        "context_inventory",
        "symbols and files fetched this session with token counts",
        "{}",
    ),
    row("investigation_suggest", "next things to investigate", "{}"),
    row(
        "symforge_retrieve",
        "the full output behind the handle",
        r#"{"hash":"{ccr_hash}"}"#,
    )
    .explicit(&["hash"]),
    row("status", "trust envelope and index health summary", "{}"),
    row(
        "symforge",
        "the routed tool's answer",
        r#"{"query":"who calls target_fn"}"#,
    ),
    row(
        "symforge_edit",
        "edit preview or confirmation",
        r#"{"path":"src/short.rs","symbol":"SHORT_A","op":"replace","body":"pub const SHORT_A: u32 = 1;"}"#,
    ),
    row(
        "secret_remediate",
        "remediation preview or result, never secret bytes",
        r#"{"scope":"repo","finding_ids":["{finding_id}"],"action":"externalize"}"#,
    ),
];

/// The contract of `tool`, if it has one.
pub(crate) fn contract_for(tool: &str) -> Option<&'static ToolContract> {
    TOOL_CONTRACTS.iter().find(|c| c.tool == tool)
}

/// Order `sections` as `tool`'s contract drops them under pressure (FR-025): the listed
/// ones first, in contract order, then any the contract does not name, in the order given.
pub(crate) fn in_drop_order<'a>(tool: &str, sections: &mut [(&'a str, &'a str)]) {
    let order = contract_for(tool).map_or(&[][..], |c| c.optional_sections);
    sections.sort_by_key(|(name, _)| {
        order
            .iter()
            .position(|listed| listed == name)
            .unwrap_or(order.len())
    });
}

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
fn element_object(value: &Value) -> Option<serde_json::Map<String, Value>> {
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
fn find_key_path(map: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
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

/// Arguments of a tool with none; every key a caller sends is unknown.
#[derive(serde::Deserialize, schemars::JsonSchema)]
struct NoArguments {}

type Probe = fn(Value) -> Result<(), StrictError>;
type SchemaNames = fn() -> Vec<String>;

fn probe<P: DeserializeOwned>(value: Value) -> Result<(), StrictError> {
    decode_strict::<P>(value).map(drop)
}

/// Property names of `T`'s published schema.
fn schema_names<T: schemars::JsonSchema>() -> Vec<String> {
    serde_json::to_value(schemars::schema_for!(T))
        .ok()
        .and_then(|schema| {
            schema
                .get("properties")
                .and_then(Value::as_object)
                .map(|properties| properties.keys().cloned().collect())
        })
        .unwrap_or_default()
}

/// The facade input flattens `StelRequest`, and serde hands a flattened struct its keys
/// after the point where `serde_ignored` can see them; judge the flattened struct itself
/// once the two harness-only keys are set aside.
fn probe_symforge_facade(mut value: Value) -> Result<(), StrictError> {
    if let Value::Object(map) = &mut value {
        map.remove("_probe_legacy_tool");
        map.remove("_probe_legacy_args");
    }
    probe::<StelRequest>(value)
}

/// The argument struct each registered tool decodes: its name, a strict decoder, and the
/// property names of that struct's schema. Tests pin that every contract row has exactly
/// one entry and that the schema names equal the router's published ones.
macro_rules! probe_table {
    ($(($name:literal, $ty:ty)),* $(,)?) => {
        static PROBES: &[(&str, Probe, SchemaNames)] = &[
            $(($name, probe::<$ty>, schema_names::<$ty>),)*
            (
                "symforge",
                probe_symforge_facade,
                schema_names::<crate::stel::SymforgeCallInput>,
            ),
        ];
    };
}

probe_table! {
    ("health", HealthInput),
    ("health_compact", NoArguments),
    ("checkpoint_now", CheckpointNowInput),
    ("index_folder", IndexFolderInput),
    ("validate_file_syntax", ValidateFileSyntaxInput),
    ("get_file_content", GetFileContentInput),
    ("get_symbol", GetSymbolInput),
    ("get_repo_map", GetRepoMapInput),
    ("get_file_context", GetFileContextInput),
    ("get_symbol_context", GetSymbolContextInput),
    ("search_symbols", SearchSymbolsInput),
    ("search_text", SearchTextInput),
    ("search_files", SearchFilesInput),
    ("search_knowledge", SearchKnowledgeInput),
    ("review_knowledge", ReviewKnowledgeInput),
    ("curate_knowledge", CurateKnowledgeInput),
    ("find_references", FindReferencesInput),
    ("find_dependents", FindDependentsInput),
    ("inspect_match", InspectMatchInput),
    ("analyze_file_impact", AnalyzeFileImpactInput),
    ("what_changed", WhatChangedInput),
    ("diff_symbols", DiffSymbolsInput),
    ("detect_impact", DetectImpactInput),
    ("explore", ExploreInput),
    ("replace_symbol_body", ReplaceSymbolBodyInput),
    ("edit_within_symbol", EditWithinSymbolInput),
    ("insert_symbol", InsertSymbolInput),
    ("delete_symbol", DeleteSymbolInput),
    ("batch_edit", BatchEditInput),
    ("batch_insert", BatchInsertInput),
    ("batch_rename", BatchRenameInput),
    ("edit_plan", EditPlanInput),
    ("ask", SmartQueryInput),
    ("conventions", NoArguments),
    ("context_inventory", NoArguments),
    ("investigation_suggest", InvestigationInput),
    ("symforge_retrieve", SymforgeRetrieveInput),
    ("status", StelStatusRequest),
    ("symforge_edit", StelEditRequest),
    ("secret_remediate", SecretRemediateInput),
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

/// A raw argument as the handlers' lenient decoders read it: null, `""` and `[]` are
/// absent (FR-003), `"true"`/`"1"` and `"false"`/`"0"` are booleans. `None` is absent.
fn normalized(value: Option<&Value>) -> Option<Value> {
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

/// An argument counts as given unless it is absent or `false`: `false` is the default of
/// every flag a condition names.
fn is_given(value: Option<&Value>) -> bool {
    !matches!(normalized(value), None | Some(Value::Bool(false)))
}

/// The controlling argument's value as a condition compares it.
fn current_value(args: &JsonObject, controller: &str) -> String {
    match normalized(args.get(controller)) {
        None => ABSENT.to_string(),
        Some(Value::String(s)) => s,
        Some(other) => other.to_string(),
    }
}

fn controller_of(when: &When) -> &'static str {
    match when {
        When::Equals(argument, _)
        | When::Unless(argument, _)
        | When::Present(argument)
        | When::Absent(argument) => argument,
    }
}

fn holds(when: &When, args: &JsonObject) -> bool {
    match when {
        When::Equals(controller, values) => {
            values.contains(&current_value(args, controller).as_str())
        }
        When::Unless(controller, values) => {
            !values.contains(&current_value(args, controller).as_str())
        }
        When::Present(controller) => is_given(args.get(*controller)),
        When::Absent(controller) => !is_given(args.get(*controller)),
    }
}

fn literal(value: &str) -> String {
    if matches!(value, "true" | "false") {
        value.to_string()
    } else {
        format!("\"{value}\"")
    }
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

/// A mode the server chose because a lone argument uses only that mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ModeSelection {
    pub controller: &'static str,
    pub mode: &'static str,
    /// The argument whose presence selected the mode.
    pub argument: &'static str,
}

/// The line that tells the caller which mode the server chose for it.
pub(crate) fn selection_note(selected: &[ModeSelection]) -> String {
    selected
        .iter()
        .map(|s| {
            format!(
                "Mode: `{}` selected {}={} for this call.",
                s.argument,
                s.controller,
                literal(s.mode)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Append [`selection_note`] to the last text block of `result`.
/// Only a tool whose output is prose may have a selecting row: a tool with a fixed
/// grammar (`review_knowledge` prints key=value lines) would be corrupted by the line, so
/// it must never get one. The audited table has no selecting row at all.
pub(crate) fn append_selection_note(result: &mut CallToolResult, selected: &[ModeSelection]) {
    if selected.is_empty() {
        return;
    }
    let note = selection_note(selected);
    for block in result.content.iter_mut().rev() {
        if let rmcp::model::ContentBlock::Text(text) = block {
            text.text.push_str(&format!("\n\n{note}"));
            return;
        }
    }
}

fn condition_refusal(
    tool: &str,
    condition: &Condition,
    args: &JsonObject,
    selected: &[ModeSelection],
) -> String {
    // No refusal echoes a value the caller sent (a path may be sensitive and a later guard
    // of the handler refuses it without echo): it names arguments, and the controller's
    // expected values from the table.
    let argument = condition.argument;
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
    match &condition.applies_when {
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

/// Enforce the contract's conditions on `args` (FR-002), in two passes so the result does
/// not depend on row order. First the mode is resolved: an argument given outside the
/// mode that uses it selects that mode when it is the only one that does and the caller
/// chose none (the controlling argument is written into `args`); two arguments selecting
/// different modes of one controller are refused. Then every row is judged against the
/// resolved mode and refused, naming the mode, when the argument is outside it. Returns
/// the modes the server chose, or the refusal text.
pub(crate) fn apply_conditions(
    tool: &str,
    args: &mut JsonObject,
) -> Result<Vec<ModeSelection>, String> {
    match contract_for(tool) {
        Some(contract) => apply_rows(tool, contract.conditions, args),
        None => Ok(Vec::new()),
    }
}

/// [`apply_conditions`] over explicit rows (the tests drive synthetic ones).
fn apply_rows(
    tool: &str,
    rows: &[Condition],
    args: &mut JsonObject,
) -> Result<Vec<ModeSelection>, String> {
    let mut selected: Vec<ModeSelection> = Vec::new();
    for condition in rows {
        if condition.on_mismatch != SelectMode || !is_given(args.get(condition.argument)) {
            continue;
        }
        let When::Equals(controller, values) = &condition.applies_when else {
            continue;
        };
        let Some(mode) = values.iter().find(|v| **v != ABSENT) else {
            continue;
        };
        if let Some(earlier) = selected.iter().find(|s| s.controller == *controller) {
            if earlier.mode != *mode {
                return Err(format!(
                    "invalid_request: \"{}\" selects {controller}={} and \"{}\" selects \
                     {controller}={}; they cannot be combined. Drop one of them.",
                    earlier.argument,
                    literal(earlier.mode),
                    condition.argument,
                    literal(mode)
                ));
            }
            continue;
        }
        if current_value(args, controller) != ABSENT {
            continue;
        }
        if condition
            .adaptive_with
            .is_some_and(|other| is_given(args.get(other)))
        {
            continue;
        }
        let value = match *mode {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            other => Value::String(other.to_string()),
        };
        args.insert((*controller).to_string(), value);
        selected.push(ModeSelection {
            controller,
            mode,
            argument: condition.argument,
        });
    }
    for condition in rows {
        if !is_given(args.get(condition.argument)) || holds(&condition.applies_when, args) {
            continue;
        }
        let controller = controller_of(&condition.applies_when);
        let controller_absent = current_value(args, controller) == ABSENT;
        if controller_absent
            && condition
                .adaptive_with
                .is_some_and(|other| is_given(args.get(other)))
        {
            continue;
        }
        return Err(condition_refusal(tool, condition, args, &selected));
    }
    Ok(selected)
}

/// The valid top-level arguments of a tool: the properties of its published schema.
fn schema_argument_names(tool: &rmcp::model::Tool) -> Vec<String> {
    tool.input_schema
        .get("properties")
        .and_then(Value::as_object)
        .map(|properties| properties.keys().cloned().collect())
        .unwrap_or_default()
}

impl SymForgeServer {
    /// The request check every `tools/call` passes before anything else (feature 036,
    /// FR-001/FR-002): a call that carries an argument its tool does not define, or one
    /// outside its mode, is answered with an invalid-request result and never reaches a
    /// handler, the repeat tracker or the daemon. A mode that a lone argument selects is
    /// written into `arguments` and returned, so the answer can say so. Decode failures
    /// that are not unknown keys (a missing or mistyped field) stay with the handler,
    /// which reports them in its own words.
    pub(crate) fn refuse_invalid_arguments(
        &self,
        tool: &str,
        arguments: &mut Option<JsonObject>,
    ) -> Result<Vec<ModeSelection>, CallToolResult> {
        let Some(args) = arguments.as_mut() else {
            return Ok(Vec::new());
        };
        let refuse = |text: String| {
            ResultStatus::new(OutcomeClass::InvalidRequest).into_call_tool_result(text)
        };
        if let Some((_, probe, _)) = PROBES.iter().find(|(name, _, _)| *name == tool)
            && let Err(StrictError::Unknown(UnknownArguments(unknown))) =
                probe(Value::Object(args.clone()))
        {
            let valid = self
                .tool_router
                .get(tool)
                .map(schema_argument_names)
                .unwrap_or_default();
            return Err(refuse(unknown_arguments_refusal(tool, &unknown, &valid)));
        }
        apply_conditions(tool, args).map_err(refuse)
    }
}

impl crate::protocol::SymForgeServer {
    /// `(tool, typical call JSON)` for every contract row, for the integration tests that
    /// drive the production `tools/call` path (feature 036).
    #[doc(hidden)]
    pub fn typical_calls_for_tests() -> Vec<(&'static str, &'static str)> {
        TOOL_CONTRACTS
            .iter()
            .map(|c| (c.tool, c.typical_call))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::init::SYMFORGE_TOOL_NAMES;
    use crate::protocol::SymForgeServer;
    use std::collections::{BTreeMap, BTreeSet};

    const CLIENT_PREFIX: &str = "mcp__symforge__";

    fn schema_arguments() -> BTreeMap<String, BTreeSet<String>> {
        SymForgeServer::tool_definitions()
            .into_iter()
            .map(|tool| {
                let args = tool
                    .input_schema
                    .get("properties")
                    .and_then(|p| p.as_object())
                    .map(|p| p.keys().cloned().collect())
                    .unwrap_or_default();
                (tool.name.to_string(), args)
            })
            .collect()
    }

    fn controlling_argument(when: &When) -> &'static str {
        controller_of(when)
    }

    /// Every registered tool has exactly one contract and no contract is for an unknown tool.
    #[test]
    fn every_registered_tool_has_exactly_one_contract() {
        for client_name in SYMFORGE_TOOL_NAMES {
            let name = client_name
                .strip_prefix(CLIENT_PREFIX)
                .unwrap_or(client_name);
            let rows = TOOL_CONTRACTS.iter().filter(|c| c.tool == name).count();
            assert_eq!(
                rows, 1,
                "tool `{name}` must have exactly one contract, has {rows}"
            );
        }
        assert_eq!(
            TOOL_CONTRACTS.len(),
            SYMFORGE_TOOL_NAMES.len(),
            "a contract names a tool that is not in SYMFORGE_TOOL_NAMES"
        );
        let router: BTreeSet<String> = schema_arguments().into_keys().collect();
        let contracts: BTreeSet<String> =
            TOOL_CONTRACTS.iter().map(|c| c.tool.to_string()).collect();
        assert_eq!(
            router, contracts,
            "contracts differ from the registered router"
        );
    }

    fn typical_args(contract: &ToolContract) -> Value {
        fn fill(value: &mut Value) {
            match value {
                Value::String(s) if s.starts_with('{') && s.ends_with('}') => *s = "x".into(),
                Value::Array(items) => items.iter_mut().for_each(fill),
                Value::Object(map) => map.values_mut().for_each(fill),
                _ => {}
            }
        }
        let mut value: Value = serde_json::from_str(contract.typical_call).expect("typical call");
        fill(&mut value);
        value
    }

    /// Every contract row has exactly one decoder, and its typical call decodes strictly.
    #[test]
    fn every_contract_row_has_one_strict_decoder_that_accepts_its_typical_call() {
        for contract in TOOL_CONTRACTS {
            let probes = PROBES
                .iter()
                .filter(|(n, _, _)| *n == contract.tool)
                .count();
            assert_eq!(probes, 1, "{}: needs exactly one decoder", contract.tool);
        }
        assert_eq!(PROBES.len(), TOOL_CONTRACTS.len());
        for contract in TOOL_CONTRACTS {
            let (_, probe, _) = PROBES.iter().find(|(n, _, _)| *n == contract.tool).unwrap();
            let args = typical_args(contract);
            if let Err(StrictError::Unknown(unknown)) = probe(args.clone()) {
                panic!(
                    "{}: typical call has unknown keys {unknown:?}",
                    contract.tool
                );
            }
            let mut bad = args;
            bad.as_object_mut()
                .unwrap()
                .insert("zz_unknown".into(), Value::from(1));
            assert!(
                matches!(probe(bad), Err(StrictError::Unknown(ref u)) if u.0 == ["zz_unknown"]),
                "{}: an extra key must be reported",
                contract.tool
            );
        }
    }

    /// Each decoder row uses the argument struct its handler takes: the property names of the
    /// row type's schema equal those the router publishes for the tool.
    #[test]
    fn every_decoder_row_matches_the_published_schema_of_its_tool() {
        let published = schema_arguments();
        for (tool, _, schema) in PROBES {
            let row: BTreeSet<String> = schema().into_iter().collect();
            assert_eq!(
                &row,
                published
                    .get(*tool)
                    .unwrap_or_else(|| panic!("{tool}: not a registered tool")),
                "{tool}: the decoder row's type differs from the handler's argument type"
            );
        }
        // The `symforge` row compares `SymforgeCallInput` (the handler's type) while its
        // decoder judges the `StelRequest` it flattens: both must publish the same
        // arguments, so the decoder judges exactly what the handler accepts (the two
        // harness-only keys aside).
        let flattened: BTreeSet<String> = schema_names::<StelRequest>().into_iter().collect();
        assert_eq!(&flattened, &published["symforge"]);
    }

    /// The wire forms the handlers' lenient decoders accept normalise the same way here.
    #[test]
    fn lenient_wire_forms_normalise_like_the_decoders_read_them() {
        let given = |v: Value| is_given(Some(&v));
        for absent in [
            Value::Null,
            Value::from(""),
            Value::from("0"),
            Value::from("false"),
            Value::from(false),
            serde_json::json!([]),
        ] {
            assert!(!given(absent.clone()), "{absent} must read as absent");
        }
        for present in [
            Value::from("1"),
            Value::from("true"),
            Value::from(true),
            Value::from(7),
        ] {
            assert!(given(present.clone()), "{present} must read as given");
        }
        let args: JsonObject = serde_json::from_value(serde_json::json!({
            "mode": "", "a": "1", "b": "0", "c": null
        }))
        .unwrap();
        assert_eq!(current_value(&args, "mode"), ABSENT);
        assert_eq!(current_value(&args, "a"), "true");
        assert_eq!(current_value(&args, "b"), "false");
        assert_eq!(current_value(&args, "c"), ABSENT);
        assert_eq!(current_value(&args, "missing"), ABSENT);
    }

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
    fn decode_strict_reports_nested_keys_and_honours_aliases() {
        let nested = serde_json::json!({
            "edits": [{ "path": "a.rs", "name": "f",
                        "operation": { "type": "delete" }, "zz": 1 }]
        });
        match decode_strict::<BatchEditInput>(nested) {
            Err(StrictError::Unknown(u)) => assert_eq!(u.0, ["edits.0.zz"]),
            other => panic!("nested key not reported: {:?}", other.err()),
        }
        let aliased = serde_json::json!({
            "path": "a.rs", "symbol": "f", "body": "fn f() {}"
        });
        assert!(decode_strict::<ReplaceSymbolBodyInput>(aliased).is_ok());
        // `deny_unknown_fields` structs refuse through serde's own error, normalised.
        let denied = serde_json::json!({ "targets": [{ "path": "a.rs", "zz": 1 }] });
        match decode_strict::<GetSymbolInput>(denied) {
            Err(StrictError::Unknown(u)) => assert_eq!(u.0, ["targets.0.zz"]),
            other => panic!("deny_unknown_fields not located: {:?}", other.err()),
        }
        // A missing required field is not an unknown key.
        assert!(matches!(
            decode_strict::<BatchEditInput>(serde_json::json!({ "dry_run": true })),
            Err(StrictError::Decode(_))
        ));
    }

    /// Each condition either selects its mode or refuses naming it, outside its mode.
    #[test]
    fn every_table_row_refuses_only_in_the_situation_it_names() {
        // A value no refusal may echo.
        let sensitive = Value::from("SENSITIVE_VALUE_123");
        for contract in TOOL_CONTRACTS {
            for c in contract.conditions {
                let controller = controller_of(&c.applies_when);
                let mut args = JsonObject::new();
                args.insert(c.argument.to_string(), sensitive.clone());
                let label = format!("{}: {}", contract.tool, c.argument);
                match &c.applies_when {
                    When::Absent(_) => {
                        let mut with = args.clone();
                        with.insert(controller.to_string(), Value::from(1));
                        let text = apply_conditions(contract.tool, &mut with)
                            .expect_err(&format!("{label} with {controller}"));
                        assert!(
                            text.contains(c.argument) && text.contains(controller),
                            "{text}"
                        );
                        assert!(!text.contains("SENSITIVE_VALUE"), "{label}: echoed a value");
                        assert!(
                            apply_conditions(contract.tool, &mut args).is_ok(),
                            "{label}"
                        );
                    }
                    When::Unless(_, values) => {
                        for value in *values {
                            let mut with = args.clone();
                            with.insert(controller.to_string(), Value::from(*value));
                            let text = apply_conditions(contract.tool, &mut with)
                                .expect_err(&format!("{label} with {controller}={value}"));
                            assert!(
                                text.contains(c.argument) && text.contains(controller),
                                "{text}"
                            );
                            assert!(!text.contains("SENSITIVE_VALUE"), "{label}: echoed a value");
                        }
                        // Every other value reaches a branch that reads the argument.
                        for other in ["Compact", "summary", "refs", "", "zz"] {
                            let mut with = args.clone();
                            with.insert(controller.to_string(), Value::from(other));
                            assert!(
                                apply_conditions(contract.tool, &mut with).is_ok(),
                                "{label}: refused for {controller}={other:?}"
                            );
                        }
                        assert!(
                            apply_conditions(contract.tool, &mut args).is_ok(),
                            "{label}"
                        );
                    }
                    other => panic!("{label}: a table row must be Absent or Unless, got {other:?}"),
                }
            }
        }
    }

    /// Selection, ordering, conflicts and adaptive rows, on synthetic rows (the audited table
    /// has none that select a mode).
    #[test]
    fn mode_selection_is_order_independent_and_conflicts_are_refused() {
        const ROWS: &[Condition] = &[
            Condition {
                argument: "limit",
                applies_when: When::Equals("flag", &["true"]),
                on_mismatch: OnMismatch::SelectMode,
                adaptive_with: None,
            },
            Condition {
                argument: "kind",
                applies_when: When::Equals("mode", &["refs", ABSENT]),
                on_mismatch: OnMismatch::Refuse,
                adaptive_with: None,
            },
            Condition {
                argument: "dir",
                applies_when: When::Equals("mode", &["impl"]),
                on_mismatch: OnMismatch::SelectMode,
                adaptive_with: None,
            },
            Condition {
                argument: "extra",
                applies_when: When::Equals("mode", &["other"]),
                on_mismatch: OnMismatch::SelectMode,
                adaptive_with: None,
            },
            Condition {
                argument: "max_files",
                applies_when: When::Equals("detail", &["full"]),
                on_mismatch: OnMismatch::SelectMode,
                adaptive_with: Some("max_tokens"),
            },
            Condition {
                argument: "depth",
                applies_when: When::Equals("detail", &["tree", "compact", ABSENT]),
                on_mismatch: OnMismatch::Refuse,
                adaptive_with: None,
            },
        ];
        let run = |json: serde_json::Value| {
            let mut args: JsonObject = serde_json::from_value(json).unwrap();
            let result = apply_rows("synthetic", ROWS, &mut args);
            (result, args)
        };
        // A lone argument selects its mode and says so.
        let (result, args) = run(serde_json::json!({ "limit": 5 }));
        assert_eq!(result.unwrap()[0].argument, "limit");
        assert_eq!(args["flag"], Value::Bool(true));
        // Chosen otherwise by the caller: refused, naming the mode.
        let (result, _) = run(serde_json::json!({ "limit": 5, "flag": "false" }));
        assert!(result.unwrap_err().contains("flag"));
        // A selected mode is judged against every row, whatever their order.
        for json in [
            serde_json::json!({ "dir": "auto", "kind": "call" }),
            serde_json::json!({ "kind": "call", "dir": "auto" }),
        ] {
            let text = run(json).0.unwrap_err();
            assert!(text.contains("`dir` selected mode=\"impl\""), "{text}");
        }
        let text = run(serde_json::json!({ "max_files": 50, "depth": 3 }))
            .0
            .unwrap_err();
        assert!(
            text.contains("`max_files` selected detail=\"full\""),
            "{text}"
        );
        // Two arguments selecting different modes of one controller.
        let text = run(serde_json::json!({ "dir": 1, "extra": 1 }))
            .0
            .unwrap_err();
        assert!(
            text.contains("\"dir\"") && text.contains("\"extra\"") && text.contains("combined")
        );
        // An adaptive row is accepted as given, and is not a selection.
        let (result, args) =
            run(serde_json::json!({ "max_files": 5, "max_tokens": 100, "depth": 2 }));
        assert!(result.unwrap().is_empty());
        assert!(!args.contains_key("detail"));
    }

    /// The published description of every conditioned argument names the argument that
    /// controls it, so the schema a client reads states the rule the check enforces.
    #[test]
    fn conditioned_arguments_name_their_controller_in_the_schema() {
        let tools = SymForgeServer::tool_definitions();
        for contract in TOOL_CONTRACTS {
            let tool = tools.iter().find(|t| t.name == contract.tool).unwrap();
            for c in contract.conditions {
                let controller = controller_of(&c.applies_when);
                let description = tool
                    .input_schema
                    .get("properties")
                    .and_then(|p| p.get(c.argument))
                    .and_then(|p| p.get("description"))
                    .and_then(Value::as_str)
                    .unwrap_or_else(|| {
                        panic!("{}: `{}` has no description", contract.tool, c.argument)
                    });
                assert!(
                    description.contains(controller),
                    "{}: the description of `{}` must name `{controller}`: {description}",
                    contract.tool,
                    c.argument
                );
            }
        }
    }

    /// A condition may name only arguments present in the schema (data-model.md), and the
    /// same holds for explicit arguments and the keys of the typical call.
    #[test]
    fn contract_arguments_are_in_the_published_schema() {
        let schemas = schema_arguments();
        for contract in TOOL_CONTRACTS {
            let schema = &schemas[contract.tool];
            let mut named: Vec<&str> = Vec::new();
            for condition in contract.conditions {
                named.push(condition.argument);
                named.push(controlling_argument(&condition.applies_when));
            }
            named.extend(contract.explicit_arguments);
            let typical: serde_json::Value = serde_json::from_str(contract.typical_call)
                .unwrap_or_else(|e| panic!("{}: typical_call is not JSON: {e}", contract.tool));
            let object = typical
                .as_object()
                .unwrap_or_else(|| panic!("{}: typical_call is not an object", contract.tool));
            named.extend(object.keys().map(String::as_str));
            for argument in named {
                assert!(
                    schema.contains(argument),
                    "{}: `{argument}` is not in the tool's input schema {schema:?}",
                    contract.tool
                );
            }
            assert!(
                !contract.primary.is_empty(),
                "{}: empty primary",
                contract.tool
            );
        }
    }
}
