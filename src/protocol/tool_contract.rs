//! Per-tool contract table (feature 036, FR-004; data-model.md `ToolContract`).
//!
//! One static row per registered tool: the conditions under which a mode-specific
//! argument applies, the content the tool exists to return, its optional sections in
//! the order they are dropped under pressure, the arguments whose presence marks
//! content as explicitly requested, and the argument set of its typical call.
//!
//! The set of valid argument names is NOT stored here; it is the tool's published input
//! schema. The tests below check that every name used in this table is in that schema.
//!
//! Written from the handlers on 2026-11-11. Where a handler's doc comment and its code
//! disagree (`get_repo_map` `path` and `max_files`, research D2) the row follows the
//! code, and T010/T020 move the row and the doc together.
// ponytail: the table has no consumer yet. T007/T008 (sectioned budget) read
// `primary`, `optional_sections` and `explicit_arguments`; T017-T020 (strict decode and
// mode check) read `conditions`. The allow goes when T008 lands.
#![allow(dead_code)]

/// Stand-in value in [`When::Equals`] for "the controlling argument is absent".
pub(crate) const ABSENT: &str = "(absent)";

/// What a condition does when the argument is given outside the mode that uses it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OnMismatch {
    /// Exactly one mode uses the argument and the caller chose none: select that mode.
    SelectMode,
    /// Refuse, naming the mode that accepts the argument.
    Refuse,
}

/// The mode in which an argument applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum When {
    /// `argument` is one of `values` (use [`ABSENT`] for "not given").
    Equals(&'static str, &'static [&'static str]),
    /// `argument` is given.
    Present(&'static str),
    /// `argument` is not given.
    Absent(&'static str),
}

/// `argument` applies only when `applies_when` holds.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Condition {
    pub argument: &'static str,
    pub applies_when: When,
    pub on_mismatch: OnMismatch,
}

#[derive(Debug)]
pub(crate) struct ToolContract {
    /// Registered tool name, without the `mcp__symforge__` client prefix.
    pub tool: &'static str,
    pub conditions: &'static [Condition],
    /// The content the tool exists to return.
    pub primary: &'static str,
    /// Sections dropped under pressure, first entry dropped first.
    pub optional_sections: &'static [&'static str],
    /// Arguments whose presence marks content as explicitly requested (FR-024).
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
    }
}

use OnMismatch::{Refuse, SelectMode};
use When::{Absent, Equals};

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
        .drops(&["quarantine registry", "savings"])
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
    .when(&[
        // Code today: `path` also filters detail="full"; two modes use it, so no SelectMode.
        cond("path", Equals("detail", &["tree", "full"]), Refuse),
        cond("depth", Equals("detail", &["tree"]), SelectMode),
        cond("max_files", Equals("detail", &["full"]), SelectMode),
    ])
    .drops(&["knowledge digests"])
    .explicit(&["detail", "path", "depth"]),
    row(
        "get_file_context",
        "outline of the file",
        r#"{"path":"src/parent/leaf.rs"}"#,
    )
    .drops(&["git", "references", "consumers", "imports"])
    .explicit(&["sections"]),
    row(
        "get_symbol_context",
        "definition with every caller and callee named",
        r#"{"name":"target_fn","path":"src/parent/leaf.rs"}"#,
    )
    .when(&[cond("file", Absent("bundle"), Refuse)])
    .drops(&["git activity", "type dependencies", "siblings", "implementations"])
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
    )
    .when(&[
        cond(
            "whole_word",
            Equals("regex", &["false", ABSENT]),
            Refuse,
        ),
        cond("follow_refs_limit", Equals("follow_refs", &["true"]), SelectMode),
    ])
    .drops(&["callers of enclosing symbols", "surrounding context"]),
    row(
        "search_files",
        "paths ranked by relevance to the query",
        r#"{"query":"leaf"}"#,
    )
    .when(&[cond(
        "anchor_path",
        Equals("rank_by", &["path+cochange"]),
        SelectMode,
    )]),
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
    .when(&[
        cond("path", Equals("mode", &["document"]), SelectMode),
        cond("path_prefix", Equals("mode", &["summary", "remediation"]), Refuse),
    ])
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
    .when(&[
        cond("direction", Equals("mode", &["implementations"]), SelectMode),
        cond("kind", Equals("mode", &["references", ABSENT]), Refuse),
        cond("path", Equals("mode", &["references", ABSENT]), Refuse),
        cond("symbol_kind", Equals("mode", &["references", ABSENT]), Refuse),
        cond("symbol_line", Equals("mode", &["references", ABSENT]), Refuse),
        cond("compact", Equals("mode", &["references", ABSENT]), Refuse),
        cond("max_per_file", Equals("mode", &["references", ABSENT]), Refuse),
    ])
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
    .drops(&["siblings"])
    .explicit(&["line"]),
    row(
        "analyze_file_impact",
        "symbol churn and dependents of the re-read file",
        r#"{"path":"src/parent/leaf.rs"}"#,
    )
    .when(&[cond(
        "co_changes_limit",
        Equals("include_co_changes", &["true"]),
        SelectMode,
    )])
    .drops(&["co-changes"]),
    row(
        "what_changed",
        "the changed paths",
        r#"{"uncommitted":true}"#,
    )
    .when(&[cond("include_symbol_diff", Absent("since"), Refuse)])
    .drops(&["symbol diff"]),
    row(
        "diff_symbols",
        "added, removed and modified symbols between the refs",
        r#"{"base":"HEAD"}"#,
    ),
    row(
        "detect_impact",
        "affected symbols or files ranked by distance",
        r#"{"base_branch":"main"}"#,
    )
    .when(&[cond("base_branch", Absent("since"), Refuse)]),
    row(
        "explore",
        "symbols, text patterns and files for the concept",
        r#"{"query":"target function"}"#,
    )
    .drops(&["call chains", "signatures"]),
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
    )
    .when(&[
        cond("occurrence", Equals("replace_all", &["false", ABSENT]), Refuse),
        cond("near_line", Equals("replace_all", &["false", ABSENT]), Refuse),
    ]),
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
    row(
        "investigation_suggest",
        "next things to investigate",
        "{}",
    ),
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
        match when {
            When::Equals(argument, _) | When::Present(argument) | When::Absent(argument) => {
                argument
            }
        }
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
