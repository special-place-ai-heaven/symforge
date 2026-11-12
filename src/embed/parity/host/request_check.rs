//! The embed lane's request check (feature 036, T021): a wire request whose tool
//! arguments carry a key the request type does not declare, a value of the wrong type,
//! or no value for a required field, is refused with the text the MCP lane prints for
//! the same tool and call (`guidance::request_refusal`).
//!
//! The wire envelope (`HostRequest`, `Query.request`, `Edit.request`, ...) is walked by
//! hand to find the object that plays the MCP tool's arguments; only that object is
//! judged, by the same strict decode the MCP lane runs, so a key is named as the MCP lane
//! names it (`zz`, `targets.0.zz`), never with envelope segments. Envelope-level mistakes
//! (an unknown variant, a bad `limits` key) have no MCP counterpart and keep the
//! untexted `InvalidRequest`.

use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::embed::parity::QueryRequest;
use crate::embed::parity::edit::WireEditRequest;
use crate::embed::parity::host::{HostHealthRequest, HostRefreshRequest, HostRequest};
use crate::embed::parity::knowledge::{CurateKnowledgeInput, ReviewKnowledgeInput};
use crate::embed::parity::stel::{StelEditRequest, StelRequest, StelStatusRequest};
use crate::embed::parity::{
    ask, changes, detect_impact, file_impact, guidance, knowledge, read, read_context, reference,
    search, symbol, symbol_context,
};
use crate::index_lifecycle::guidance::argument_values::{
    check_values, notes_suffix, notes_tokens, value_refusal,
};
use crate::index_lifecycle::guidance::request_refusal::{
    StrictError, UnknownArguments, UnknownKey, decode_strict_wrapped, embed_decode_failure_text,
    embed_request_tool, embed_unknown_arguments_refusal, expected_fields_in,
};

/// A key no request type declares: decoding `{SENTINEL: null}` makes a
/// `deny_unknown_fields` type list the fields it declares. Never echoed.
const SENTINEL: &str = "\u{1}";

/// Strict and plain decoders of one request type. `strict` takes the struct variant the
/// arguments are wrapped in, when they are one.
#[derive(Clone, Copy)]
struct Codec {
    strict: fn(Value, Option<&str>) -> Result<(), StrictError>,
    plain: fn(Value) -> Option<String>,
}

fn codec<P: DeserializeOwned>() -> Codec {
    Codec {
        strict: |value, variant| {
            decode_strict_wrapped::<P>(value, &|args| variant.map(|name| json!({ name: args })))
                .map(drop)
        },
        plain: |value| {
            serde_json::from_value::<P>(value)
                .err()
                .map(|e| e.to_string())
        },
    }
}

/// What the strict decode found wrong with a request's arguments.
enum Verdict {
    Fine,
    Unknown(UnknownArguments),
    /// A decode failure that is not an unknown key: serde's message.
    Decode(String),
}

/// The part of a wire request that plays an MCP tool's arguments.
struct Located {
    /// Wire label, `EMBED_REQUEST_TOOLS` key.
    label: String,
    args: Value,
    /// `None`: a unit variant, which takes no arguments.
    codec: Option<Codec>,
    /// A struct variant is decoded as `{variant: args}` of its enum.
    variant: Option<String>,
    /// An internally tagged request keeps its `kind` tag when probed for its fields.
    tag: Option<String>,
}

impl Located {
    fn decodable(&self, args: Value) -> Value {
        match &self.variant {
            Some(variant) => json!({ variant.as_str(): args }),
            None => args,
        }
    }

    /// The strict decode's verdict on the arguments; a unit variant takes none.
    fn verdict(&self) -> Verdict {
        let Some(codec) = self.codec else {
            let keys: Vec<UnknownKey> = self
                .args
                .as_object()
                .map(|object| {
                    object
                        .keys()
                        .map(|key| UnknownKey(vec![key.clone()]))
                        .collect()
                })
                .unwrap_or_default();
            return if keys.is_empty() {
                Verdict::Fine
            } else {
                Verdict::Unknown(UnknownArguments {
                    keys,
                    incomplete: false,
                })
            };
        };
        match (codec.strict)(self.args.clone(), self.variant.as_deref()) {
            Ok(()) => Verdict::Fine,
            Err(StrictError::Unknown(unknown)) => Verdict::Unknown(unknown),
            Err(StrictError::Decode(error)) => Verdict::Decode(error.to_string()),
        }
    }

    /// The fields the request type declares, in declaration order, when it refuses
    /// unknown keys; `None` for a type that accepts them (its fields are the MCP tool's).
    fn declared_fields(&self) -> Option<Vec<String>> {
        let Some(codec) = self.codec else {
            return Some(Vec::new());
        };
        let mut probe = serde_json::Map::new();
        if let Some(tag) = &self.tag {
            probe.insert("kind".to_string(), Value::String(tag.clone()));
        }
        probe.insert(SENTINEL.to_string(), Value::Null);
        let message = (codec.plain)(self.decodable(Value::Object(probe)))?;
        expected_fields_in(&message)
    }
}

/// The one `(variant, payload)` of an externally tagged value.
fn single(value: &Value) -> Option<(&str, &Value)> {
    let object = value.as_object()?;
    if object.len() != 1 {
        return None;
    }
    object
        .iter()
        .next()
        .map(|(key, value)| (key.as_str(), value))
}

fn payload<P: DeserializeOwned>(label: String, args: &Value) -> Located {
    Located {
        label,
        args: args.clone(),
        codec: Some(codec::<P>()),
        variant: None,
        tag: None,
    }
}

fn struct_variant<E: DeserializeOwned>(label: String, variant: &str, args: &Value) -> Located {
    Located {
        label,
        args: args.clone(),
        codec: Some(codec::<E>()),
        variant: Some(variant.to_string()),
        tag: None,
    }
}

fn unit(label: String, args: &Value) -> Located {
    Located {
        label,
        args: args.clone(),
        codec: None,
        variant: None,
        tag: None,
    }
}

fn locate_query(request: &Value) -> Option<Located> {
    let (variant, args) = single(request)?;
    let label = format!("Query.{variant}");
    Some(match variant {
        "TextSearch" => payload::<search::TextSearchRequest>(label, args),
        "SymbolSearch" => payload::<search::SymbolSearchRequest>(label, args),
        "FileSearch" => payload::<search::FileSearchRequest>(label, args),
        "SearchKnowledge" => payload::<knowledge::search::SearchKnowledgeRequest>(label, args),
        "FileContent" => payload::<read::FileContentRequest>(label, args),
        "RepoMap" => payload::<read_context::RepoMapRequest>(label, args),
        "FileContext" => payload::<read_context::FileContextRequest>(label, args),
        "SourcePage" => payload::<read::SourcePageRequest>(label, args),
        "SymbolRead" => payload::<symbol::SymbolReadRequest>(label, args),
        "SymbolContext" => payload::<symbol_context::SymbolContextRequest>(label, args),
        "InspectMatch" => payload::<symbol::InspectMatchRequest>(label, args),
        "ReferenceSearch" => payload::<reference::ReferenceSearchRequest>(label, args),
        "DependentSearch" => payload::<reference::DependentSearchRequest>(label, args),
        "FileImpact" => payload::<file_impact::FileImpactRequest>(label, args),
        "Explore" => payload::<guidance::ExploreRequest>(label, args),
        "WhatChanged" => payload::<changes::WhatChangedRequest>(label, args),
        "DiffSymbols" => payload::<changes::DiffSymbolsRequest>(label, args),
        "DetectImpact" => payload::<detect_impact::DetectImpactRequest>(label, args),
        "Ask" => payload::<ask::AskRequest>(label, args),
        "Symforge" => payload::<StelRequest>(label, args),
        "File"
        | "Symbol"
        | "Context"
        | "References"
        | "Dependents"
        | "SearchSymbols"
        | "SearchText"
        | "SearchFiles"
        | "Graph"
        | "Syntax"
        | "Diff"
        | "Impact"
        | "InvestigationSuggest"
        | "EditPlan"
        | "Retrieve" => struct_variant::<QueryRequest>(label, variant, args),
        "Conventions" | "ContextInventory" => unit(label, args),
        _ => return None,
    })
}

fn locate(wire: &Value) -> Option<Located> {
    let (variant, args) = single(wire)?;
    let label = variant.to_string();
    Some(match variant {
        "StatusReport" => payload::<StelStatusRequest>(label, args),
        "HealthReport" => payload::<HostHealthRequest>(label, args),
        "SymforgeEdit" => payload::<StelEditRequest>(label, args),
        "RefreshWith" => payload::<HostRefreshRequest>(label, args),
        "Status" | "Health" | "HealthCompact" | "Catalog" | "Refresh" => unit(label, args),
        "Checkpoint" | "InspectSecrets" | "PreviewSecretRemediation" | "ApplySecretRemediation" => {
            struct_variant::<HostRequest>(label, variant, args)
        }
        "Query" => return locate_query(args.get("request")?),
        "Edit" => {
            let request = args.get("request")?;
            let kind = request.get("kind")?.as_str()?;
            Located {
                label: format!("Edit.{kind}"),
                args: request.clone(),
                codec: Some(codec::<WireEditRequest>()),
                variant: None,
                tag: Some(kind.to_string()),
            }
        }
        "Knowledge" => {
            let request = args.get("request")?;
            let kind = request.get("kind")?.as_str()?;
            let input = request.get("input")?;
            let label = format!("Knowledge.{kind}");
            match kind {
                "review" => payload::<ReviewKnowledgeInput>(label, input),
                "curate_preview" | "curate_apply" => payload::<CurateKnowledgeInput>(label, input),
                _ => return None,
            }
        }
        _ => return None,
    })
}

/// The refusal text for `wire`, when its tool arguments carry a key the request type
/// does not declare or fail to decode; `None` otherwise (including every envelope-level
/// mistake).
pub(super) fn argument_refusal(wire: &Value) -> Option<String> {
    let located = locate(wire)?;
    let verdict = located.verdict();
    if let Verdict::Unknown(unknown) = &verdict {
        return Some(embed_unknown_arguments_refusal(
            &located.label,
            unknown,
            located.declared_fields().as_deref(),
        ));
    }
    // Then the argument values, as on the MCP lane, before a closed embed enum's decode
    // failure can speak for them. A query that decodes is judged on the typed lane
    // instead (`embed_query::typed_value_refusal`), which keeps the refusal kind hosts
    // already match on (`UnsupportedOption` for a name outside its set).
    let typed_lane_judges = located.label.starts_with("Query.") && matches!(verdict, Verdict::Fine);
    if !typed_lane_judges
        && let (Some(tool), Some(args)) =
            (embed_request_tool(&located.label), located.args.as_object())
        && let Some(text) = value_refusal(tool, args)
    {
        return Some(text);
    }
    match verdict {
        Verdict::Fine | Verdict::Unknown(_) => None,
        Verdict::Decode(message) => Some(embed_decode_failure_text(
            &located.label,
            &message,
            located.declared_fields().as_deref(),
        )),
    }
}

/// The capped-value notes the answer to `wire` must carry, already in the answer's
/// grammar (`argument_values::notes_suffix`); empty when nothing was capped or the
/// request answers for no MCP tool.
pub(super) fn argument_notes(wire: &Value) -> String {
    let Some(located) = locate(wire) else {
        return String::new();
    };
    match (embed_request_tool(&located.label), located.args.as_object()) {
        (Some(tool), Some(args)) => check_values(tool, args)
            .map(|notes| notes_suffix(tool, args, &notes))
            .unwrap_or_default(),
        _ => String::new(),
    }
}

/// Reduce the caller's `max_tokens` in `wire` by the note's share, as the MCP lane does,
/// so the answer plus its note fits the budget the caller set.
pub(super) fn reserve_for_notes(wire: &mut Value, notes: &str) {
    let tokens = notes_tokens(notes);
    let args = match wire {
        Value::Object(map) if map.len() == 1 => map.values_mut().next(),
        _ => None,
    };
    let args = match args {
        Some(Value::Object(query)) if query.contains_key("request") => {
            query.get_mut("request").and_then(|request| match request {
                Value::Object(variant) if variant.len() == 1 => variant.values_mut().next(),
                _ => None,
            })
        }
        other => other,
    };
    if let Some(Value::Object(args)) = args
        && let Some(budget) = args.get("max_tokens").and_then(Value::as_u64)
    {
        args.insert(
            "max_tokens".to_string(),
            Value::from(budget.saturating_sub(tokens).max(1)),
        );
    }
}

/// Append `notes` (already in the answer's grammar) to the answer text of a serialized
/// reply, as the MCP lane appends them to its last text block: the shallowest `rendered`
/// or `report` string. A reply with no answer text (symbol and file search) carries them
/// in `freshness_notes`, the field for lines MCP prints after its answer.
// ponytail: walks the serialized reply; a typed `notes` field on every output if more
// kinds of note appear.
pub(super) fn append_notes(reply: &mut Value, notes: &str) {
    if notes.is_empty() {
        return;
    }
    for keys in [&["rendered", "report"][..], &["freshness_notes"][..]] {
        let mut level: Vec<&mut Value> = vec![&mut *reply];
        while !level.is_empty() {
            let mut next = Vec::new();
            for value in level {
                let Value::Object(map) = value else {
                    if let Value::Array(items) = value {
                        next.extend(items.iter_mut());
                    }
                    continue;
                };
                if let Some(key) = keys.iter().find(|key| {
                    map.get(**key).is_some_and(|v| {
                        v.is_string() || (v.is_null() && **key == "freshness_notes")
                    })
                }) {
                    let current = map.get(*key).and_then(Value::as_str).unwrap_or_default();
                    let text = if current.is_empty() {
                        notes.trim_start_matches('\n').to_string()
                    } else {
                        format!("{current}{notes}")
                    };
                    map.insert((*key).to_string(), Value::String(text));
                    return;
                }
                next.extend(map.values_mut());
            }
            level = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index_lifecycle::guidance::request_refusal::{EMBED_REQUEST_TOOLS, tool_arguments};

    /// A minimal wire request for `label`, carrying `args` as its tool arguments.
    fn wire_for(label: &str, args: Value) -> Value {
        let limits = json!({ "max_results": 1, "max_bytes": 1 });
        match label.split_once('.') {
            Some(("Query", variant)) => {
                json!({ "Query": { "request": { variant: args }, "limits": limits } })
            }
            Some(("Edit", kind)) => {
                let mut request = args;
                request["kind"] = json!(kind);
                json!({ "Edit": { "request": request, "operation_key": null } })
            }
            Some(("Knowledge", kind)) => json!({ "Knowledge": {
                "request": { "kind": kind, "input": args },
                "operation_key": null,
            }}),
            _ => json!({ label: args }),
        }
    }

    /// Every labelled request is found by the walk, except the two that take no tool
    /// arguments at all (resource and prompt reads).
    #[test]
    fn every_labelled_request_is_located() {
        for (label, _) in EMBED_REQUEST_TOOLS {
            let located = locate(&wire_for(label, json!({ "zz_unknown": 1 })));
            if matches!(*label, "Resource" | "Prompt") {
                assert!(located.is_none(), "{label}");
                continue;
            }
            let located = located.unwrap_or_else(|| panic!("{label} is not located"));
            assert_eq!(located.label, *label);
            assert!(
                matches!(located.verdict(), Verdict::Unknown(u) if u.paths().contains(&"zz_unknown".to_string())),
                "{label}: the unknown key was not found"
            );
        }
    }

    /// Every embed request that answers for an MCP tool, with the fields it declares where
    /// they differ from the tool's published arguments: `(label, only on embed, only on
    /// MCP)`. A request that accepts unknown keys is the MCP tool's own type and has no
    /// row. Pinned so a new difference is a reviewed change.
    #[test]
    fn field_set_differences_are_pinned() {
        let mut differences = Vec::new();
        for (label, tool) in EMBED_REQUEST_TOOLS {
            let (Some(tool), Some(located)) = (tool, locate(&wire_for(label, json!({})))) else {
                continue;
            };
            let Some(fields) = located.declared_fields() else {
                continue;
            };
            let published = tool_arguments(tool).unwrap_or_default();
            let only_embed: Vec<&str> = fields
                .iter()
                .map(String::as_str)
                .filter(|f| !published.contains(f))
                .collect();
            let only_mcp: Vec<&str> = published
                .iter()
                .copied()
                .filter(|p| !fields.iter().any(|f| f == p))
                .collect();
            if !only_embed.is_empty() || !only_mcp.is_empty() {
                differences.push(format!(
                    "{label} ({tool}): embed only [{}]; mcp only [{}]",
                    only_embed.join(", "),
                    only_mcp.join(", ")
                ));
            }
        }
        let expected: &[&str] = &[
            "Query.TextSearch (search_text): embed only []; mcp only [project, projects]",
            "Query.SymbolSearch (search_symbols): embed only []; mcp only [project, projects]",
            "Query.FileSearch (search_files): embed only []; mcp only [project, projects]",
            "Query.FileContent (get_file_content): embed only []; mcp only [project]",
            "Query.Syntax (validate_file_syntax): embed only []; mcp only [project]",
            "Query.FileImpact (analyze_file_impact): embed only []; mcp only [project]",
            "Query.Explore (explore): embed only []; mcp only [project]",
            "Query.InvestigationSuggest (investigation_suggest): embed only []; mcp only [project]",
            "Query.EditPlan (edit_plan): embed only []; mcp only [project]",
            "Query.DetectImpact (detect_impact): embed only [paths]; mcp only []",
            "Edit.plan (edit_plan): embed only []; mcp only [project]",
        ];
        assert_eq!(
            differences,
            expected,
            "field-set differences changed:\n{}",
            differences.join("\n")
        );
    }

    #[test]
    fn a_unit_request_with_arguments_is_refused_like_a_tool_without_any() {
        let text = argument_refusal(&json!({ "Query": {
            "request": { "Conventions": { "zz_unknown": 1 } },
            "limits": { "max_results": 1, "max_bytes": 1 }
        }}));
        assert_eq!(
            text.as_deref(),
            Some(
                "invalid_request: conventions does not accept \"zz_unknown\". Valid arguments: \
                 none."
            )
        );
    }

    #[test]
    fn envelope_mistakes_carry_no_text() {
        assert_eq!(argument_refusal(&json!({ "NoSuchRequest": {} })), None);
        assert_eq!(
            argument_refusal(&json!({ "Query": {
                "request": { "RepoMap": {} },
                "limits": { "max_results": 1, "max_bytes": 1, "zz_unknown": 1 }
            }})),
            None
        );
    }

    #[test]
    fn a_refusal_never_echoes_a_value() {
        let text = argument_refusal(&json!({ "Query": {
            "request": { "RepoMap": { "zz_unknown": "secret-value-must-not-echo" } },
            "limits": { "max_results": 1, "max_bytes": 1 }
        }}))
        .expect("refused");
        assert!(!text.contains("secret-value-must-not-echo"), "{text}");
    }
}
