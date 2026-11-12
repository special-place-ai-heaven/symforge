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

pub(crate) use crate::index_lifecycle::guidance::request_refusal::{
    ABSENT, ModeSelection, StrictError, When, decode_strict, from_value_noting, note_keys_of,
    unknown_arguments_refusal,
};
use crate::index_lifecycle::guidance::request_refusal::{
    condition_refusal, conflicting_selection_refusal, current_value, literal, normalized,
};
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

/// An argument counts as given unless it is absent or `false`: `false` is the default of
/// every flag a condition names.
fn is_given(value: Option<&Value>) -> bool {
    !matches!(normalized(value), None | Some(Value::Bool(false)))
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
                return Err(conflicting_selection_refusal(
                    controller,
                    earlier,
                    condition.argument,
                    mode,
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
        return Err(condition_refusal(
            tool,
            condition.argument,
            &condition.applies_when,
            args,
            &selected,
        ));
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
            && let Err(StrictError::Unknown(unknown)) = probe(Value::Object(args.clone()))
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
                matches!(probe(bad), Err(StrictError::Unknown(ref u)) if u.paths() == ["zz_unknown"]),
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

    #[test]
    fn decode_strict_reports_nested_keys_and_honours_aliases() {
        let nested = serde_json::json!({
            "edits": [{ "path": "a.rs", "name": "f",
                        "operation": { "type": "delete" }, "zz": 1 }]
        });
        match decode_strict::<BatchEditInput>(nested) {
            Err(StrictError::Unknown(u)) => assert_eq!(u.paths(), ["edits.0.zz"]),
            other => panic!("nested key not reported: {:?}", other.err()),
        }
        let aliased = serde_json::json!({
            "path": "a.rs", "symbol": "f", "body": "fn f() {}"
        });
        assert!(decode_strict::<ReplaceSymbolBodyInput>(aliased).is_ok());
        // `deny_unknown_fields` structs refuse through serde's own error, normalised.
        let denied = serde_json::json!({ "targets": [{ "path": "a.rs", "zz": 1 }] });
        match decode_strict::<GetSymbolInput>(denied) {
            Err(StrictError::Unknown(u)) => assert_eq!(u.paths(), ["targets.0.zz"]),
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
