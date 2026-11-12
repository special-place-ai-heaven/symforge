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

use super::request_refusal::{array_items, capped_key, current_value, element_object};

/// What an argument accepts.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Accepts {
    /// One of these strings, exactly.
    OneOf(&'static [&'static str]),
    /// One of these strings, in any letter case (the handler compares caselessly).
    OneOfCaseless(&'static [&'static str]),
    /// An integer of at least `min`, and at most `max` when one is given.
    Range(u64, Option<u64>),
    /// An integer of at least `min`; above `max` the tool uses `max`, as its published
    /// description says, and the answer says so ([`check_values`] returns the note).
    Capped(u64, u64),
}

use Accepts::{Capped, OneOf, OneOfCaseless, Range};

/// The `sections` element the MCP decoders add for `include_tests=true`
/// (`protocol::read_tools::encode_include_tests_marker`). A caller never sends it; the
/// daemon route sees it in the arguments the front end re-serializes, so it is not judged.
pub(crate) const INCLUDE_TESTS_SECTION_MARKER: &str = "__symforge_include_tests";

/// When a row is judged: the handler reads the argument only in some modes, and an
/// argument a mode ignores is not judged here (the mode rules own that).
#[derive(Clone, Copy, Debug)]
pub(crate) enum Applies {
    Always,
    /// Not when `argument` is `value` (as the mode rules compare it).
    Unless(&'static str, &'static str),
    /// Only when `argument` is `value`.
    If(&'static str, &'static str),
}

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
    pub applies: Applies,
}

const fn row(tool: &'static str, argument: &'static str, accepts: Accepts) -> ValueRow {
    ValueRow {
        tool,
        argument,
        accepts,
        applies: Applies::Always,
    }
}

impl ValueRow {
    const fn unless(mut self, argument: &'static str, value: &'static str) -> Self {
        self.applies = Applies::Unless(argument, value);
        self
    }
    const fn only_if(mut self, argument: &'static str, value: &'static str) -> Self {
        self.applies = Applies::If(argument, value);
        self
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
    // "default 2, max 5"; 0 groups every file under the scope root (`map_dir_key`).
    row("get_repo_map", "depth", Capped(0, 5)),
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
    // The daemon's `trace_symbol` alias (judged as `get_symbol_context` plus this row).
    row("trace_symbol", "kind", OneOfCaseless(SYMBOL_KINDS)),
    // -- search --
    row("search_symbols", "kind", OneOfCaseless(SYMBOL_KINDS)),
    row("search_symbols", "limit", Capped(1, 100)),
    row("search_symbols", "max_tokens", POSITIVE),
    row("search_text", "limit", Range(1, Some(10_000))).unless("group_by", "count"),
    row("search_text", "max_per_file", Range(1, Some(10_000))).unless("group_by", "count"),
    row("search_text", "follow_refs_limit", POSITIVE).only_if("follow_refs", "true"),
    row("search_text", "group_by", OneOf(TEXT_GROUPINGS)),
    row("search_text", "max_tokens", POSITIVE),
    row("search_files", "limit", Capped(1, 50)),
    row("search_files", "max_tokens", POSITIVE),
    row("search_knowledge", "limit", Capped(1, 100)),
    // "Server-bounded" by the published description (`knowledge::review` MAX_LIMIT).
    row("review_knowledge", "limit", Capped(1, 100)),
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
    row("find_references", "symbol_line", POSITIVE).unless("mode", "implementations"),
    // "capped at 100/500" and "capped at 50" by the published description.
    row("find_references", "limit", Capped(1, 100)).unless("mode", "implementations"),
    row("find_references", "limit", Capped(1, 500)).only_if("mode", "implementations"),
    row("find_references", "max_per_file", Capped(1, 50)).unless("mode", "implementations"),
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
    row("find_dependents", "limit", Capped(1, 100)),
    row("find_dependents", "max_per_file", Capped(1, 50)),
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
    row("health", "quarantine_limit", Capped(1, 1000)),
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

/// A value outside an argument's set or range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ValueRefusal {
    pub text: String,
    /// The value is outside a closed set of names (an unsupported option), rather than
    /// a number out of range or an empty name.
    pub closed_set: bool,
}

/// Judge `args` of MCP tool `tool`: the first argument outside its accepted values is
/// refused; otherwise every capped value that was above its cap gets one note the answer
/// must carry (`Note: "limit" = 500 is above its maximum of 100; 100 was used.`). Null,
/// `""` and `[]` are absent (FR-003); a value of the wrong JSON type is the decoder's.
pub(crate) fn check_values(
    tool: &str,
    args: &Map<String, Value>,
) -> Result<Vec<String>, ValueRefusal> {
    if let Some(text) = blank_name_refusal(tool, args) {
        return Err(ValueRefusal {
            text,
            closed_set: false,
        });
    }
    let mut notes = Vec::new();
    for row in ARGUMENT_VALUES.iter().filter(|row| row.tool == tool) {
        if !applies(row.applies, args) {
            continue;
        }
        for (path, value) in values_of(row.argument, args) {
            match judge(tool, &path, row.accepts, &value) {
                Judged::Valid => {}
                Judged::Capped(note) => notes.push(note),
                Judged::Refused(text) => {
                    return Err(ValueRefusal {
                        text,
                        closed_set: matches!(row.accepts, OneOf(_) | OneOfCaseless(_)),
                    });
                }
            }
        }
    }
    Ok(notes)
}

/// [`check_values`]' refusal text only.
pub(crate) fn value_refusal(tool: &str, args: &Map<String, Value>) -> Option<String> {
    check_values(tool, args).err().map(|refusal| refusal.text)
}

/// The notes [`check_values`] returns, appended to an answer's text.
pub(crate) fn with_notes(mut text: String, notes: &[String]) -> String {
    for note in notes {
        text.push_str("\n\n");
        text.push_str(note);
    }
    text
}

fn blank_name_refusal(tool: &str, args: &Map<String, Value>) -> Option<String> {
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
    None
}

fn applies(applies: Applies, args: &Map<String, Value>) -> bool {
    match applies {
        Applies::Always => true,
        Applies::Unless(argument, value) => current_value(args, argument) != value,
        Applies::If(argument, value) => current_value(args, argument) == value,
    }
}

/// Every value `argument` names in `args`, with the path the refusal prints for it.
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
        .filter(|(_, item)| item.as_str() != Some(INCLUDE_TESTS_SECTION_MARKER))
        .filter_map(|(index, item)| match field {
            None => Some((format!("{array}.{index}"), item)),
            Some(field) => element_object(&item)?
                .get(field)
                .map(|value| (format!("{array}.{index}.{field}"), value.clone())),
        })
        .collect()
}

fn listed(values: &[&str]) -> String {
    values
        .iter()
        .map(|value| format!("\"{value}\""))
        .collect::<Vec<_>>()
        .join(", ")
}

/// An integer exactly as the lenient decoders accept one (`serde_input::lenient_u32`):
/// a non-negative JSON integer or a string of digits, untrimmed. Anything else is the
/// decoder's to refuse, in its own words.
fn integer(value: &Value) -> Option<u64> {
    match value {
        Value::Number(number) => number.as_u64(),
        Value::String(text) => text.parse::<u64>().ok(),
        _ => None,
    }
}

enum Judged {
    Valid,
    Capped(String),
    Refused(String),
}

fn range_text(min: u64, max: Option<u64>) -> String {
    match max {
        Some(max) => format!("{min} to {max}"),
        None => format!("{min} or greater"),
    }
}

fn judge(tool: &str, path: &str, accepts: Accepts, value: &Value) -> Judged {
    let refused_number = |number: u64, max: Option<u64>, min: u64| {
        Judged::Refused(format!(
            "invalid_request: {tool} does not accept \"{path}\" = {}. Valid range: {}.",
            capped_key(&number.to_string()),
            range_text(min, max)
        ))
    };
    match accepts {
        OneOf(values) | OneOfCaseless(values) => {
            let Value::String(text) = value else {
                return Judged::Valid;
            };
            if text.is_empty() {
                return Judged::Valid;
            }
            let caseless = matches!(accepts, OneOfCaseless(_));
            let known = values.iter().any(|valid| {
                *valid == text.as_str() || (caseless && valid.eq_ignore_ascii_case(text))
            });
            if known {
                Judged::Valid
            } else {
                Judged::Refused(format!(
                    "invalid_request: {tool} does not accept that value of \"{path}\". Valid \
                     values: {}.",
                    listed(values)
                ))
            }
        }
        Range(min, max) => match integer(value) {
            Some(number) if number < min || max.is_some_and(|max| number > max) => {
                refused_number(number, max, min)
            }
            _ => Judged::Valid,
        },
        Capped(min, max) => match integer(value) {
            Some(number) if number < min => refused_number(number, None, min),
            Some(number) if number > max => Judged::Capped(format!(
                "Note: \"{path}\" = {} is above its maximum of {max}; {max} was used.",
                capped_key(&number.to_string())
            )),
            _ => Judged::Valid,
        },
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
        for row in ARGUMENT_VALUES
            .iter()
            .filter(|row| row.tool != "trace_symbol")
        {
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
