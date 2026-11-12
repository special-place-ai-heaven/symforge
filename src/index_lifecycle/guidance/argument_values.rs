//! The accepted VALUES of every tool argument that has a closed set or a range (feature
//! 036, owner rule "no argument may be silently ignored"): one table, judged before any
//! work by every lane an LLM talks to, the MCP server (`protocol::tool_contract`, the
//! daemon route) and the embed host (`HostRoom::dispatch_wire`, the typed query lane), so
//! a mistyped `kind` or a `limit` of 0 is refused with the same bytes everywhere instead
//! of answering a different question.
//!
//! A row refuses a value the handler would otherwise replace with its default, ignore, or
//! clamp without saying so. Not here: values a handler already refuses with its own text
//! (`inspect_match` line past the end, `insert_symbol` position, `secret_remediate`
//! action, typed enums the decoder refuses), and upper bounds a tool's published
//! description promises to cap (`capped at 100`): those stay clamps.
//!
//! A refusal names the argument and lists the valid values or the valid range. It echoes
//! a NUMBER the caller sent (a number cannot carry a secret worth more than its digits,
//! and it is cut at the key cap) but never a string value.

use serde_json::{Map, Value};

use super::request_refusal::{array_items, capped_key, element_object};

/// What an argument accepts.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Accepts {
    /// One of these strings, exactly.
    OneOf(&'static [&'static str]),
    /// One of these strings, in any letter case (the handler compares caselessly).
    OneOfCaseless(&'static [&'static str]),
    /// An integer of at least `min`, and at most `max` when one is given.
    Range(u64, Option<u64>),
}

use Accepts::{OneOf, OneOfCaseless, Range};

/// At least one: a count, a 1-based line, or a token budget.
const POSITIVE: Accepts = Range(1, None);

/// Symbol kind filters: every `SymbolKind` display name, the aliases
/// `kind_filter_matches` accepts, and `all` (`live_index::search`).
pub(crate) const SYMBOL_KINDS: &[&str] = &[
    "all",
    "fn",
    "function",
    "method",
    "class",
    "struct",
    "enum",
    "interface",
    "mod",
    "module",
    "const",
    "constant",
    "let",
    "variable",
    "type",
    "trait",
    "impl",
    "other",
    "key",
    "section",
    "macro-generated",
];

/// `get_file_context` `sections` (also the published schema's item enum).
pub(crate) const FILE_CONTEXT_SECTIONS: &[&str] = &[
    "outline",
    "imports",
    "consumers",
    "references",
    "git",
    "knowledge",
];

/// `get_symbol_context` `sections` (also the published schema's item enum).
pub(crate) const SYMBOL_CONTEXT_SECTIONS: &[&str] = &[
    "dependents",
    "siblings",
    "implementations",
    "git",
    "knowledge",
];

/// `get_file_content` `mode` (`read_contract::file_content_options_from_input`).
pub(crate) const FILE_CONTENT_MODES: &[&str] = &["lines", "symbol", "match", "chunk"];

/// `search_text` `group_by` (`search_render`; `purpose` is the alias of `usage`).
pub(crate) const TEXT_GROUPINGS: &[&str] =
    &["file", "symbol", "usage", "purpose", "names", "count"];

/// `get_symbol_context` `verbosity` (`symbol_context::apply_verbosity`).
const VERBOSITIES: &[&str] = &["full", "summary", "signature", "compact"];

/// One argument's accepted values. `argument` is a top-level name, `name[]` for every
/// element of an array argument, or `name[].field` for a field of every element.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ValueRow {
    pub tool: &'static str,
    pub argument: &'static str,
    pub accepts: Accepts,
}

const fn row(tool: &'static str, argument: &'static str, accepts: Accepts) -> ValueRow {
    ValueRow {
        tool,
        argument,
        accepts,
    }
}

/// Every judged argument, in the order a call is judged (the first failing row refuses).
pub(crate) static ARGUMENT_VALUES: &[ValueRow] = &[
    // -- reads --
    row("get_file_content", "mode", OneOf(FILE_CONTENT_MODES)),
    row("get_file_content", "start_line", POSITIVE),
    row("get_file_content", "end_line", POSITIVE),
    row("get_file_content", "around_line", POSITIVE),
    row("get_file_content", "chunk_index", POSITIVE),
    row("get_file_content", "max_lines", POSITIVE),
    row("get_file_content", "match_occurrence", POSITIVE),
    row("get_file_content", "symbol_line", POSITIVE),
    row("get_file_content", "limit", POSITIVE),
    row("get_file_content", "max_tokens", POSITIVE),
    row("get_symbol", "kind", OneOfCaseless(SYMBOL_KINDS)),
    row("get_symbol", "symbol_line", POSITIVE),
    row("get_symbol", "targets[].kind", OneOfCaseless(SYMBOL_KINDS)),
    row("get_symbol", "targets[].symbol_line", POSITIVE),
    row("get_symbol", "max_tokens", POSITIVE),
    row(
        "get_repo_map",
        "detail",
        OneOf(&["compact", "full", "tree"]),
    ),
    row("get_repo_map", "depth", Range(0, Some(5))),
    row("get_repo_map", "max_files", POSITIVE),
    row("get_repo_map", "max_tokens", POSITIVE),
    row(
        "get_file_context",
        "sections[]",
        OneOfCaseless(FILE_CONTEXT_SECTIONS),
    ),
    row("get_file_context", "max_tokens", POSITIVE),
    row(
        "get_symbol_context",
        "symbol_kind",
        OneOfCaseless(SYMBOL_KINDS),
    ),
    row("get_symbol_context", "symbol_line", POSITIVE),
    row("get_symbol_context", "verbosity", OneOf(VERBOSITIES)),
    row(
        "get_symbol_context",
        "sections[]",
        OneOfCaseless(SYMBOL_CONTEXT_SECTIONS),
    ),
    row("get_symbol_context", "max_tokens", POSITIVE),
    row("inspect_match", "max_tokens", POSITIVE),
    // -- search --
    row("search_symbols", "kind", OneOfCaseless(SYMBOL_KINDS)),
    // Capped at 100 by the published description: a clamp, not a refusal.
    row("search_symbols", "limit", POSITIVE),
    row("search_symbols", "max_tokens", POSITIVE),
    row("search_text", "limit", Range(1, Some(10_000))),
    row("search_text", "max_per_file", Range(1, Some(10_000))),
    row("search_text", "follow_refs_limit", POSITIVE),
    row("search_text", "group_by", OneOf(TEXT_GROUPINGS)),
    row("search_text", "max_tokens", POSITIVE),
    // Capped at 50 by the published description.
    row("search_files", "limit", POSITIVE),
    row("search_files", "max_tokens", POSITIVE),
    row("search_knowledge", "limit", Range(1, Some(100))),
    // "Server-bounded" by the published description.
    row("review_knowledge", "limit", POSITIVE),
    row("explore", "depth", Range(1, Some(3))),
    row("explore", "limit", Range(1, Some(10_000))),
    row("explore", "max_tokens", POSITIVE),
    // -- references --
    row(
        "find_references",
        "kind",
        OneOf(super::reference_read::REFERENCE_KIND_NAMES),
    ),
    row(
        "find_references",
        "symbol_kind",
        OneOfCaseless(SYMBOL_KINDS),
    ),
    row("find_references", "symbol_line", POSITIVE),
    // Capped at 100/500 and 50 by the published description.
    row("find_references", "limit", POSITIVE),
    row("find_references", "max_per_file", POSITIVE),
    row(
        "find_references",
        "mode",
        OneOf(&["references", "implementations"]),
    ),
    row(
        "find_references",
        "direction",
        OneOf(&["trait", "type", "auto"]),
    ),
    row("find_references", "max_tokens", POSITIVE),
    // Capped at 100 and 50 by the published description.
    row("find_dependents", "limit", POSITIVE),
    row("find_dependents", "max_per_file", POSITIVE),
    row(
        "find_dependents",
        "format",
        OneOf(&["text", "mermaid", "dot"]),
    ),
    row("find_dependents", "max_tokens", POSITIVE),
    // -- changes, impact, guidance --
    row("analyze_file_impact", "co_changes_limit", POSITIVE),
    row("what_changed", "max_tokens", POSITIVE),
    row("diff_symbols", "max_tokens", POSITIVE),
    row("ask", "max_tokens", POSITIVE),
    // The published schema's minimum.
    row("symforge", "max_tokens", Range(64, None)),
    // Capped at 1000 by the published description.
    row("health", "quarantine_limit", POSITIVE),
    // -- edits (MCP request shapes; the embed edit requests have their own) --
    row("replace_symbol_body", "kind", OneOfCaseless(SYMBOL_KINDS)),
    row("replace_symbol_body", "symbol_line", POSITIVE),
    row("insert_symbol", "kind", OneOfCaseless(SYMBOL_KINDS)),
    row("insert_symbol", "symbol_line", POSITIVE),
    row("delete_symbol", "kind", OneOfCaseless(SYMBOL_KINDS)),
    row("delete_symbol", "symbol_line", POSITIVE),
    row("edit_within_symbol", "kind", OneOfCaseless(SYMBOL_KINDS)),
    row("edit_within_symbol", "symbol_line", POSITIVE),
    row("edit_within_symbol", "near_line", POSITIVE),
    row("batch_edit", "edits[].kind", OneOfCaseless(SYMBOL_KINDS)),
    row("batch_edit", "edits[].symbol_line", POSITIVE),
    row(
        "batch_insert",
        "targets[].kind",
        OneOfCaseless(SYMBOL_KINDS),
    ),
    row("batch_insert", "targets[].symbol_line", POSITIVE),
    row("batch_rename", "kind", OneOfCaseless(SYMBOL_KINDS)),
    row("batch_rename", "symbol_line", POSITIVE),
];

/// A required string argument whose empty (or blank) value names nothing: refused, unless
/// `unless` is given (the call then selects by that argument instead). `defaulted`: the
/// decoder fills an absent value with `""`, so absence is judged here too; otherwise an
/// absent value is the decoder's missing-field refusal.
#[derive(Clone, Copy, Debug)]
pub(crate) struct NonEmptyRow {
    pub tool: &'static str,
    pub argument: &'static str,
    pub unless: Option<&'static str>,
    pub defaulted: bool,
}

pub(crate) static NON_EMPTY: &[NonEmptyRow] = &[
    NonEmptyRow {
        tool: "get_symbol",
        argument: "name",
        unless: Some("targets"),
        defaulted: true,
    },
    NonEmptyRow {
        tool: "get_symbol_context",
        argument: "name",
        unless: None,
        defaulted: false,
    },
    NonEmptyRow {
        tool: "find_references",
        argument: "name",
        unless: None,
        defaulted: false,
    },
];

/// The refusal text for the first argument of `args` outside its accepted values, for
/// MCP tool `tool`; `None` when every judged argument is valid. Null, `""` and `[]` are
/// absent (FR-003). A value of the wrong JSON type is left to the decoder.
pub(crate) fn value_refusal(tool: &str, args: &Map<String, Value>) -> Option<String> {
    for rule in NON_EMPTY.iter().filter(|rule| rule.tool == tool) {
        let blank = match args.get(rule.argument) {
            None | Some(Value::Null) => rule.defaulted,
            Some(Value::String(text)) => text.trim().is_empty(),
            Some(_) => false,
        };
        let excused = rule
            .unless
            .and_then(|other| args.get(other))
            .and_then(array_items)
            .is_some_and(|items| !items.is_empty());
        if blank && !excused {
            let unless = rule
                .unless
                .map(|other| format!(" unless \"{other}\" is given"))
                .unwrap_or_default();
            return Some(format!(
                "invalid_request: {tool} requires a non-empty \"{}\"{unless}.",
                rule.argument
            ));
        }
    }
    ARGUMENT_VALUES
        .iter()
        .filter(|row| row.tool == tool)
        .find_map(|row| row_refusal(tool, row, args))
}

/// Every value `row` names in `args`, with the path the refusal prints for it.
fn values_of(argument: &str, args: &Map<String, Value>) -> Vec<(String, Value)> {
    let Some((array, field)) = argument.split_once("[]") else {
        return args
            .get(argument)
            .map(|value| vec![(argument.to_string(), value.clone())])
            .unwrap_or_default();
    };
    let Some(items) = args.get(array).and_then(array_items) else {
        return Vec::new();
    };
    let field = field.strip_prefix('.');
    items
        .into_iter()
        .enumerate()
        .filter_map(|(index, item)| match field {
            None => Some((format!("{array}.{index}"), item)),
            Some(field) => element_object(&item)?
                .get(field)
                .map(|value| (format!("{array}.{index}.{field}"), value.clone())),
        })
        .collect()
}

fn row_refusal(tool: &str, row: &ValueRow, args: &Map<String, Value>) -> Option<String> {
    values_of(row.argument, args)
        .into_iter()
        .find_map(|(path, value)| judge(tool, &path, row.accepts, &value))
}

fn listed(values: &[&str]) -> String {
    values
        .iter()
        .map(|value| format!("\"{value}\""))
        .collect::<Vec<_>>()
        .join(", ")
}

/// An integer as the lenient decoders read it: a JSON integer or a string of digits
/// (`"5"`, `"-1"`). `None` for anything else, which the decoder judges.
fn integer(value: &Value) -> Option<i128> {
    match value {
        Value::Number(number) => number
            .as_u64()
            .map(i128::from)
            .or_else(|| number.as_i64().map(i128::from)),
        Value::String(text) => text.trim().parse::<i128>().ok(),
        _ => None,
    }
}

fn judge(tool: &str, path: &str, accepts: Accepts, value: &Value) -> Option<String> {
    match accepts {
        OneOf(values) | OneOfCaseless(values) => {
            let Value::String(text) = value else {
                return None;
            };
            if text.is_empty() {
                return None;
            }
            let caseless = matches!(accepts, OneOfCaseless(_));
            let known = values.iter().any(|valid| {
                *valid == text.as_str() || (caseless && valid.eq_ignore_ascii_case(text))
            });
            (!known).then(|| {
                format!(
                    "invalid_request: {tool} does not accept that value of \"{path}\". Valid \
                     values: {}.",
                    listed(values)
                )
            })
        }
        Range(min, max) => {
            if matches!(value, Value::String(text) if text.is_empty()) {
                return None;
            }
            let number = integer(value)?;
            let inside =
                number >= i128::from(min) && max.is_none_or(|max| number <= i128::from(max));
            (!inside).then(|| {
                let range = match max {
                    Some(max) => format!("{min} to {max}"),
                    None => format!("{min} or greater"),
                };
                format!(
                    "invalid_request: {tool} does not accept \"{path}\" = {}. Valid range: \
                     {range}.",
                    capped_key(&number.to_string())
                )
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::SymbolKind;
    use serde_json::json;

    fn refusal(tool: &str, args: Value) -> Option<String> {
        value_refusal(tool, args.as_object().expect("object"))
    }

    #[test]
    fn a_value_outside_the_set_is_refused_with_the_set() {
        assert_eq!(
            refusal(
                "find_dependents",
                json!({ "path": "a.rs", "format": "bogus" })
            )
            .as_deref(),
            Some(
                "invalid_request: find_dependents does not accept that value of \"format\". \
                 Valid values: \"text\", \"mermaid\", \"dot\"."
            )
        );
        assert_eq!(refusal("find_dependents", json!({ "format": "dot" })), None);
    }

    #[test]
    fn a_number_outside_the_range_is_refused_with_the_range() {
        assert_eq!(
            refusal("explore", json!({ "query": "x", "depth": 4 })).as_deref(),
            Some("invalid_request: explore does not accept \"depth\" = 4. Valid range: 1 to 3.")
        );
        assert_eq!(
            refusal("find_references", json!({ "name": "x", "limit": "0" })).as_deref(),
            Some(
                "invalid_request: find_references does not accept \"limit\" = 0. Valid range: 1 \
                 or greater."
            )
        );
        assert_eq!(
            refusal("explore", json!({ "query": "x", "depth": 3 })),
            None
        );
    }

    #[test]
    fn absent_values_and_wrong_types_are_not_judged_here() {
        for args in [
            json!({ "name": "x", "kind": null }),
            json!({ "name": "x", "kind": "" }),
            json!({ "name": "x", "limit": "" }),
            json!({ "name": "x", "limit": "many" }),
            json!({ "name": "x", "kind": 5 }),
        ] {
            assert_eq!(refusal("find_references", args.clone()), None, "{args}");
        }
    }

    #[test]
    fn elements_and_element_fields_are_named_by_their_path() {
        assert_eq!(
            refusal(
                "get_symbol_context",
                json!({ "name": "x", "sections": ["git", "bogus"] })
            )
            .as_deref(),
            Some(
                "invalid_request: get_symbol_context does not accept that value of \
                 \"sections.1\". Valid values: \"dependents\", \"siblings\", \
                 \"implementations\", \"git\", \"knowledge\"."
            )
        );
        let stringified =
            json!({ "targets": "[{\"path\":\"a.rs\",\"name\":\"x\",\"symbol_line\":0}]" });
        assert_eq!(
            refusal("get_symbol", stringified).as_deref(),
            Some(
                "invalid_request: get_symbol does not accept \"targets.0.symbol_line\" = 0. \
                 Valid range: 1 or greater."
            )
        );
        assert_eq!(
            refusal(
                "get_symbol_context",
                json!({ "name": "x", "sections": ["GIT"] })
            ),
            None,
            "sections compare caselessly, as the handlers do"
        );
    }

    #[test]
    fn a_blank_required_name_is_refused_unless_targets_select() {
        assert_eq!(
            refusal("find_references", json!({ "name": " " })).as_deref(),
            Some("invalid_request: find_references requires a non-empty \"name\".")
        );
        assert_eq!(
            refusal("get_symbol", json!({ "path": "a.rs", "name": "" })).as_deref(),
            Some(
                "invalid_request: get_symbol requires a non-empty \"name\" unless \"targets\" \
                 is given."
            )
        );
        assert_eq!(
            refusal(
                "get_symbol",
                json!({ "targets": [{ "path": "a.rs", "name": "x" }] })
            ),
            None
        );
    }

    #[test]
    fn a_refusal_never_echoes_a_string_value() {
        let text = refusal(
            "search_text",
            json!({ "query": "x", "group_by": "secret-value" }),
        )
        .expect("refused");
        assert!(!text.contains("secret-value"), "{text}");
    }

    #[test]
    fn every_row_names_a_published_argument() {
        for row in ARGUMENT_VALUES {
            let top = row.argument.split("[]").next().unwrap_or(row.argument);
            let published = super::super::request_refusal::tool_arguments(row.tool)
                .unwrap_or_else(|| panic!("{} has no published arguments", row.tool));
            assert!(
                published.contains(&top),
                "{}.{} is not a published argument",
                row.tool,
                row.argument
            );
        }
    }

    /// Every symbol kind, by an exhaustive match: a new kind fails to compile here.
    fn every_symbol_kind() -> Vec<SymbolKind> {
        let all = [
            SymbolKind::Function,
            SymbolKind::Method,
            SymbolKind::Class,
            SymbolKind::Struct,
            SymbolKind::Enum,
            SymbolKind::Interface,
            SymbolKind::Module,
            SymbolKind::Constant,
            SymbolKind::Variable,
            SymbolKind::Type,
            SymbolKind::Trait,
            SymbolKind::Impl,
            SymbolKind::Other,
            SymbolKind::Key,
            SymbolKind::Section,
            SymbolKind::MacroGenerated,
        ];
        for kind in &all {
            match kind {
                SymbolKind::Function
                | SymbolKind::Method
                | SymbolKind::Class
                | SymbolKind::Struct
                | SymbolKind::Enum
                | SymbolKind::Interface
                | SymbolKind::Module
                | SymbolKind::Constant
                | SymbolKind::Variable
                | SymbolKind::Type
                | SymbolKind::Trait
                | SymbolKind::Impl
                | SymbolKind::Other
                | SymbolKind::Key
                | SymbolKind::Section
                | SymbolKind::MacroGenerated => {}
            }
        }
        all.to_vec()
    }

    #[test]
    fn the_symbol_kind_set_is_the_filter_the_index_applies() {
        let kinds = every_symbol_kind();
        for kind in &kinds {
            assert!(
                SYMBOL_KINDS.contains(&kind.to_string().as_str()),
                "{kind} is missing from SYMBOL_KINDS"
            );
        }
        for name in SYMBOL_KINDS {
            assert!(
                *name == "all"
                    || kinds
                        .iter()
                        .any(|kind| crate::live_index::search::kind_filter_matches(name, kind)),
                "{name} matches no symbol kind"
            );
        }
    }

    #[test]
    fn the_reference_kind_set_is_the_filter_the_handlers_apply() {
        use super::super::reference_read::{REFERENCE_KIND_FILTERS, REFERENCE_KIND_NAMES};
        let names: Vec<&str> = REFERENCE_KIND_FILTERS
            .iter()
            .map(|(name, _)| *name)
            .collect();
        assert_eq!(names, REFERENCE_KIND_NAMES);
    }

    #[test]
    fn the_file_content_modes_are_the_modes_the_reader_dispatches() {
        for mode in FILE_CONTENT_MODES {
            let input: super::super::read_contract::GetFileContentInput =
                serde_json::from_value(json!({ "path": "a.rs", "mode": mode })).expect("input");
            let outcome = super::super::read_contract::file_content_options_from_input(&input);
            assert!(
                !outcome
                    .as_ref()
                    .err()
                    .is_some_and(|error| error.starts_with("Unknown mode")),
                "{mode}: {outcome:?}"
            );
        }
    }

    #[cfg(feature = "embed")]
    #[test]
    fn the_embed_enums_accept_exactly_the_published_names() {
        use crate::embed::parity::search::TextGrouping;
        for name in TEXT_GROUPINGS {
            serde_json::from_value::<TextGrouping>(json!(name))
                .unwrap_or_else(|error| panic!("{name}: {error}"));
        }
        for grouping in [
            TextGrouping::File,
            TextGrouping::Symbol,
            TextGrouping::Usage,
            TextGrouping::Names,
            TextGrouping::Count,
        ] {
            let name = serde_json::to_value(grouping).expect("name");
            assert!(
                TEXT_GROUPINGS.contains(&name.as_str().expect("string")),
                "{name}"
            );
        }
    }
}
