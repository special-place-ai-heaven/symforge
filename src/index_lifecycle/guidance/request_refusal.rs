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

/// One key of a call that no argument struct consumed, as path segments from the top of
/// the arguments (`["edits", "0", "zz"]`). Printed joined with `.`; addressed by segment,
/// so a key that itself contains `.` is still one segment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UnknownKey(pub Vec<String>);

impl UnknownKey {
    #[cfg(test)]
    fn single(key: &str) -> Self {
        Self(vec![key.to_string()])
    }

    /// The key as the refusal prints it.
    pub(crate) fn path(&self) -> String {
        self.0.join(".")
    }
}

/// The keys of a call that no argument struct of the tool consumed, in the order the
/// decode met them. `incomplete`: the search stopped at [`MAX_DECODE_ROUNDS`] with keys
/// still unconsumed, so more exist than are listed.
#[derive(Debug)]
pub(crate) struct UnknownArguments {
    pub keys: Vec<UnknownKey>,
    pub incomplete: bool,
}

impl UnknownArguments {
    /// The keys as the refusal prints them.
    pub(crate) fn paths(&self) -> Vec<String> {
        self.keys.iter().map(UnknownKey::path).collect()
    }
}

impl std::fmt::Display for UnknownArguments {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown arguments: {}", self.paths().join(", "))
    }
}

impl std::error::Error for UnknownArguments {}

/// Why [`decode_strict`] did not return a value.
#[derive(Debug)]
pub(crate) enum StrictError {
    /// Keys that serde did not consume (aliases, nested objects and `flatten` are
    /// honoured by construction), and `unknown field` errors of `deny_unknown_fields`
    /// structs, each located in the call.
    Unknown(UnknownArguments),
    /// Any other decode failure: a wrong type, a missing field, or an unknown field whose
    /// place in the call cannot be established. Worded by [`decode_failure_text`].
    Decode(serde_json::Error),
}

/// Strip the `element N: ` prefixes a lenient container adds; the innermost index.
fn element_prefixes(message: &str) -> (Option<usize>, &str) {
    let mut index = None;
    let mut rest = message;
    while let Some(tail) = rest.strip_prefix("element ") {
        let Some((number, after)) = tail.split_once(": ") else {
            break;
        };
        let Ok(number) = number.parse::<usize>() else {
            break;
        };
        index = Some(number);
        rest = after;
    }
    (index, rest)
}

/// `element 2: unknown field `x`, expected ...` -> (Some(2), "x"); without the element
/// prefix -> (None, "x"). Only a message that IS an unknown-field error counts: a type
/// error whose value happens to contain the words is not one.
fn unknown_field_in(message: &str) -> Option<(Option<usize>, String)> {
    let (index, rest) = element_prefixes(message);
    let rest = rest.strip_prefix("unknown field `")?;
    let key = rest.split('`').next()?.to_string();
    Some((index, key))
}

/// The field names a `deny_unknown_fields` decode error lists as expected, in declaration
/// order: `unknown field `x`, expected one of `a`, `b`` -> [a, b]; `there are no fields`
/// -> []. `None` when the error is not an unknown-field error.
#[cfg(any(feature = "embed", test))]
pub(crate) fn expected_fields_in(message: &str) -> Option<Vec<String>> {
    let (_, rest) = element_prefixes(message);
    let rest = rest.strip_prefix("unknown field `")?;
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

/// A key a lenient container saw unconsumed: the element index and the key's segments.
type NestedKey = (usize, Vec<String>);

thread_local! {
    /// Ignored keys of array elements that a lenient container decoded on its own
    /// (`lenient_vec_required`), where `serde_ignored` cannot see them: the element index
    /// and the key's segments inside it. `Some` only while a strict decode runs on this
    /// thread; the decode is synchronous.
    static NESTED_IGNORED: std::cell::RefCell<Option<Vec<NestedKey>>> =
        const { std::cell::RefCell::new(None) };
    /// The array element a lenient container is decoding, for `note_value`.
    static ELEMENT_INDEX: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Switches the thread's nested-key sink on for one decode attempt.
struct NestedSink;

impl NestedSink {
    fn install() -> Self {
        NESTED_IGNORED.with(|sink| *sink.borrow_mut() = Some(Vec::new()));
        Self
    }

    fn take(&self) -> Vec<NestedKey> {
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

/// The JSON path a `serde_ignored` path names: map keys and array indices. `Some` and
/// newtype wrappers have no JSON level of their own and add no segment.
fn path_segments(path: &serde_ignored::Path<'_>) -> Vec<String> {
    use serde_ignored::Path;
    match path {
        Path::Root => Vec::new(),
        Path::Seq { parent, index } => {
            let mut segments = path_segments(parent);
            segments.push(index.to_string());
            segments
        }
        Path::Map { parent, key } => {
            let mut segments = path_segments(parent);
            segments.push(key.clone());
            segments
        }
        Path::Some { parent }
        | Path::NewtypeStruct { parent }
        | Path::NewtypeVariant { parent } => path_segments(parent),
    }
}

/// `serde_json::from_value::<T>` for element `index` of an array that a lenient container
/// decodes itself. While a strict decode is running, keys `T` does not consume are
/// recorded under the element so the strict decode can refuse them; otherwise it is
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
        ignored.push((index, path_segments(&path)));
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
pub(crate) fn array_items(value: &Value) -> Option<Vec<Value>> {
    match value {
        Value::Array(items) => Some(items.clone()),
        Value::String(text) => serde_json::from_str::<Vec<Value>>(text).ok(),
        _ => None,
    }
}

/// An array element that is an object, or a string holding one.
pub(crate) fn element_object(value: &Value) -> Option<Map<String, Value>> {
    match value {
        Value::Object(map) => Some(map.clone()),
        Value::String(text) => match serde_json::from_str::<Value>(text).ok()? {
            Value::Object(map) => Some(map),
            _ => None,
        },
        _ => None,
    }
}

/// Segments of the first object member named `key`, searching nested objects.
fn find_key_path(map: &Map<String, Value>, key: &str) -> Option<Vec<String>> {
    if map.contains_key(key) {
        return Some(vec![key.to_string()]);
    }
    map.iter().find_map(|(name, value)| match value {
        Value::Object(inner) => find_key_path(inner, key).map(|rest| {
            let mut segments = vec![name.clone()];
            segments.extend(rest);
            segments
        }),
        _ => None,
    })
}

/// Locate `key` (segments relative to array element `index`) in the call:
/// `edits.1.operation.zz`. `None` when no top-level array element holds it.
fn qualify(index: usize, key: &[String], root: &Value) -> Option<Vec<String>> {
    let first = key.first()?;
    root.as_object()?.iter().find_map(|(field, value)| {
        let items = array_items(value)?;
        let element = element_object(items.get(index)?)?;
        let inner = if element.contains_key(first) {
            key.to_vec()
        } else if let [single] = key {
            find_key_path(&element, single)?
        } else {
            return None;
        };
        let mut segments = vec![field.clone(), index.to_string()];
        segments.extend(inner);
        Some(segments)
    })
}

/// A lenient container's key where no array element holds it: `[index].key`, as the
/// MCP lane has always printed it. Not addressable, so the search stops after it.
fn unplaced(index: usize, key: Vec<String>) -> UnknownKey {
    let mut segments = vec![format!("[{index}]")];
    segments.extend(key);
    UnknownKey(segments)
}

/// Every member named `key`, at any depth, in document order: segments from the root.
/// At most [`MAX_CANDIDATES`].
fn members_named(value: &Value, key: &str) -> Vec<Vec<String>> {
    fn walk(value: &Value, key: &str, prefix: &mut Vec<String>, out: &mut Vec<Vec<String>>) {
        if out.len() >= MAX_CANDIDATES {
            return;
        }
        match value {
            Value::Object(map) => {
                if map.contains_key(key) {
                    let mut path = prefix.clone();
                    path.push(key.to_string());
                    out.push(path);
                }
                for (name, inner) in map {
                    prefix.push(name.clone());
                    walk(inner, key, prefix, out);
                    prefix.pop();
                }
            }
            Value::Array(items) => {
                for (index, inner) in items.iter().enumerate() {
                    prefix.push(index.to_string());
                    walk(inner, key, prefix, out);
                    prefix.pop();
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    // A top-level member first: it is where a caller's key most often is.
    if value.as_object().is_some_and(|map| map.contains_key(key)) {
        out.push(vec![key.to_string()]);
    }
    let mut found = Vec::new();
    walk(value, key, &mut Vec::new(), &mut found);
    for path in found {
        if !out.contains(&path) && out.len() < MAX_CANDIDATES {
            out.push(path);
        }
    }
    out
}

/// Remove the member at `segments` from `value`; whether one was removed.
fn remove_member(value: &mut Value, segments: &[String]) -> bool {
    let Some((last, parents)) = segments.split_last() else {
        return false;
    };
    let mut current = value;
    for segment in parents {
        current = match current {
            Value::Object(map) => match map.get_mut(segment) {
                Some(next) => next,
                None => return false,
            },
            Value::Array(items) => {
                match segment.parse::<usize>().ok().and_then(|i| items.get_mut(i)) {
                    Some(next) => next,
                    None => return false,
                }
            }
            _ => return false,
        };
    }
    current
        .as_object_mut()
        .is_some_and(|map| map.remove(last).is_some())
}

/// Decode attempts one strict decode makes at most: a `deny_unknown_fields` struct
/// reports one key per attempt, so each found key is removed and the decode repeated.
/// Bounds the work on a request of maximum size.
pub(crate) const MAX_DECODE_ROUNDS: usize = 32;

/// Places a key is tried at when a `deny_unknown_fields` error names it without a path.
const MAX_CANDIDATES: usize = 8;

/// One decode of `value` (wrapped by `wrap` when the type expects an envelope around it),
/// with every key `serde_ignored` and the lenient containers saw unconsumed.
fn attempt<P: DeserializeOwned>(
    value: &Value,
    wrap: &dyn Fn(&Value) -> Option<Value>,
) -> (Result<P, serde_json::Error>, Vec<UnknownKey>) {
    let mut ignored = Vec::new();
    let sink = NestedSink::install();
    let wrapped = wrap(value);
    let decoded =
        serde_ignored::deserialize::<_, _, P>(wrapped.as_ref().unwrap_or(value), |path| {
            ignored.push(UnknownKey(path_segments(&path)));
        });
    for (index, key) in sink.take() {
        ignored.push(match qualify(index, &key, value) {
            Some(segments) => UnknownKey(segments),
            None => unplaced(index, key),
        });
    }
    (decoded, ignored)
}

/// Where in `value` the `deny_unknown_fields` error naming `key` comes from. A lenient
/// container's element index places it directly; otherwise every member named `key` is a
/// candidate, and when there are several the one whose removal changes the error is it.
/// `None` when no place can be established: the refusal then makes no key-level claim.
fn place_denied<P: DeserializeOwned>(
    index: Option<usize>,
    key: &str,
    value: &Value,
    wrap: &dyn Fn(&Value) -> Option<Value>,
) -> Option<UnknownKey> {
    if let Some(index) = index {
        return Some(match qualify(index, &[key.to_string()], value) {
            Some(segments) => UnknownKey(segments),
            None => unplaced(index, vec![key.to_string()]),
        });
    }
    let candidates = members_named(value, key);
    if let [only] = candidates.as_slice() {
        return Some(UnknownKey(only.clone()));
    }
    candidates.into_iter().find_map(|candidate| {
        let mut without = value.clone();
        remove_member(&mut without, &candidate);
        let (decoded, _) = attempt::<P>(&without, wrap);
        let same = decoded
            .err()
            .and_then(|error| unknown_field_in(&error.to_string()))
            .is_some_and(|(i, k)| i.is_none() && k == key);
        (!same).then_some(UnknownKey(candidate))
    })
}

/// Decode `value` as `P` and report every key that no field of `P` consumed (FR-001), in
/// the order the decode meets them, up to [`MAX_DECODE_ROUNDS`] decodes.
///
/// Leniency about VALUE types lives in `P`'s own deserializers and is untouched; only
/// keys are judged. Paths are dotted (`edits.0.zz`); the first segment is a top-level
/// argument.
pub(crate) fn decode_strict<P: DeserializeOwned>(value: Value) -> Result<P, StrictError> {
    decode_strict_wrapped(value, &|_| None)
}

/// [`decode_strict`] for arguments that `P` reads inside an envelope: `wrap` builds the
/// decoded value from the arguments (`{variant: args}`); keys are named relative to the
/// arguments themselves.
pub(crate) fn decode_strict_wrapped<P: DeserializeOwned>(
    value: Value,
    wrap: &dyn Fn(&Value) -> Option<Value>,
) -> Result<P, StrictError> {
    let mut work: Option<Value> = None;
    let mut keys: Vec<UnknownKey> = Vec::new();
    let unknown = |keys: Vec<UnknownKey>, incomplete| {
        StrictError::Unknown(UnknownArguments {
            keys: in_document_order(&value, keys),
            incomplete,
        })
    };
    for round in 0..=MAX_DECODE_ROUNDS {
        let current = work.as_ref().unwrap_or(&value);
        let (decoded, ignored) = attempt::<P>(current, wrap);
        let found = if ignored.is_empty() {
            match decoded {
                Ok(decoded) if keys.is_empty() => return Ok(decoded),
                Ok(_) => return Err(unknown(keys, false)),
                Err(error) => {
                    let placed = unknown_field_in(&error.to_string())
                        .and_then(|(index, key)| place_denied::<P>(index, &key, current, wrap));
                    match placed {
                        Some(key) => vec![key],
                        None if keys.is_empty() => return Err(StrictError::Decode(error)),
                        None => return Err(unknown(keys, false)),
                    }
                }
            }
        } else {
            ignored
        };
        if round == MAX_DECODE_ROUNDS {
            return Err(unknown(keys, true));
        }
        let work = work.get_or_insert_with(|| value.clone());
        let removed = found
            .iter()
            .filter(|key| remove_member(work, &key.0))
            .count();
        keys.extend(found);
        if removed == 0 {
            return Err(unknown(keys, false));
        }
    }
    Err(unknown(keys, true))
}

/// `keys` in the order their members appear in `value`, so both lanes list them alike
/// however their decoders met them. A key that cannot be placed goes last.
fn in_document_order(value: &Value, mut keys: Vec<UnknownKey>) -> Vec<UnknownKey> {
    let position = |key: &UnknownKey| {
        let mut current = Some(value);
        key.0
            .iter()
            .map(|segment| {
                let (index, next) = match current {
                    Some(Value::Object(map)) => map
                        .iter()
                        .enumerate()
                        .find(|(_, (name, _))| *name == segment)
                        .map_or((usize::MAX, None), |(i, (_, inner))| (i, Some(inner))),
                    Some(Value::Array(items)) => segment
                        .parse::<usize>()
                        .ok()
                        .and_then(|i| items.get(i).map(|inner| (i, Some(inner))))
                        .unwrap_or((usize::MAX, None)),
                    _ => (usize::MAX, None),
                };
                current = next;
                index
            })
            .collect::<Vec<_>>()
    };
    keys.sort_by_cached_key(position);
    keys
}

/// An echoed argument NAME is cut to this many characters.
const KEY_ECHO_LIMIT: usize = 40;

/// A refusal names at most this many keys, then `(+N more)`.
const KEY_LIST_LIMIT: usize = 8;

pub(crate) fn capped_key(key: &str) -> String {
    if key.chars().count() <= KEY_ECHO_LIMIT {
        return key.to_string();
    }
    let cut: String = key.chars().take(KEY_ECHO_LIMIT).collect();
    format!("{cut}...")
}

/// Refusal text for keys no tool argument consumed (contracts/tool-outcome.md).
pub(crate) fn unknown_arguments_refusal(
    tool: &str,
    unknown: &UnknownArguments,
    valid: &[String],
) -> String {
    let mut named = unknown
        .keys
        .iter()
        .take(KEY_LIST_LIMIT)
        .map(|key| format!("\"{}\"", capped_key(&key.path())))
        .collect::<Vec<_>>()
        .join(", ");
    let beyond = unknown.keys.len().saturating_sub(KEY_LIST_LIMIT);
    if unknown.incomplete {
        named.push_str(&format!(" (+{beyond} or more)"));
    } else if beyond > 0 {
        named.push_str(&format!(" (+{beyond} more)"));
    }
    let valid = if valid.is_empty() {
        "none".to_string()
    } else {
        valid.join(", ")
    };
    let mut text =
        format!("invalid_request: {tool} does not accept {named}. Valid arguments: {valid}.");
    if let Some(parent) = unknown
        .keys
        .iter()
        .find_map(|key| (key.0.len() > 1).then(|| &key.0[0]))
    {
        text.push_str(&format!(
            " Check the fields of \"{}\" against the tool's input schema.",
            capped_key(parent)
        ));
    }
    text
}

// ── decode failures that are not unknown keys ──────────────────────────────────────

/// `get_symbol_context(bundle=true)` without `path`, on both lanes.
pub(crate) const BUNDLE_REQUIRES_PATH: &str = "Error: bundle=true requires the 'path' parameter.";
/// `search_files` with an empty `query` and no `changed_with`, on both lanes.
pub(crate) const SEARCH_FILES_REQUIRES_QUERY: &str =
    "search_files requires a non-empty `query` (or use `changed_with` to find co-changing files).";
/// `explore` with an empty query, on both lanes and in the facade.
pub(crate) const EXPLORE_REQUIRES_QUERY: &str = "Explore requires a non-empty query.";
/// `ask` with an empty question, on both lanes and in the facade.
pub(crate) const ASK_REQUIRES_QUESTION: &str = "query requires a non-empty question.";

/// rmcp's prefix for a tool-argument decode failure (`missing field`, wrong type); the
/// embed lane uses the same words.
pub(crate) const DECODE_FAILURE_PREFIX: &str = "failed to deserialize parameters:";

/// `message` with every argument VALUE serde quotes in it removed: `invalid type:
/// integer `42`` -> `invalid type: integer`, `string "x"` -> `string`, `unknown variant
/// `x`` -> `unknown variant`. Field names (`missing field `path``) stay.
pub(crate) fn redact_values(message: &str) -> String {
    const QUOTED: &[&str] = &[
        "boolean `",
        "integer `",
        "floating point `",
        "character `",
        "unknown variant `",
    ];
    let mut out = String::with_capacity(message.len());
    let mut rest = message;
    'scan: while !rest.is_empty() {
        for prefix in QUOTED {
            if let Some(tail) = rest.strip_prefix(prefix) {
                out.push_str(&prefix[..prefix.len() - 2]);
                // A backtick inside the value (a character value of `) is not its end
                // only when the closing one follows at once.
                let end = tail.find("`, ").or_else(|| tail.rfind('`'));
                rest = end.map_or("", |end| &tail[end + 1..]);
                continue 'scan;
            }
        }
        if let Some(tail) = rest.strip_prefix("string \"") {
            out.push_str("string");
            let mut escaped = false;
            let mut end = tail.len();
            for (offset, ch) in tail.char_indices() {
                match ch {
                    _ if escaped => escaped = false,
                    '\\' => escaped = true,
                    '"' => {
                        end = offset + 1;
                        break;
                    }
                    _ => {}
                }
            }
            rest = &tail[end..];
            continue;
        }
        let mut chars = rest.chars();
        if let Some(ch) = chars.next() {
            out.push(ch);
        }
        rest = chars.as_str();
    }
    out
}

/// The text a decode failure that is not an unknown key gets on both lanes: rmcp's
/// prefix, serde's message without values, and the tool's required fields when it has
/// any.
pub(crate) fn decode_failure_text(tool: &str, serde_message: &str, required: &[&str]) -> String {
    let mut text = format!("{DECODE_FAILURE_PREFIX} {}", redact_values(serde_message));
    if !required.is_empty() {
        text.push_str(&format!(
            "\nRequired fields for `{tool}`: {}.",
            required.join(", ")
        ));
    }
    text
}

// ── mode-condition refusals (FR-002)───────────────────────────────────────────────

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

/// The MCP tool the embed request labelled `label` answers for ([`EMBED_REQUEST_TOOLS`]).
#[cfg(feature = "embed")]
pub(crate) fn embed_request_tool(label: &str) -> Option<&'static str> {
    EMBED_REQUEST_TOOLS
        .iter()
        .find(|(name, _)| *name == label)
        .and_then(|(_, tool)| *tool)
}

/// The MCP tool `label` answers for, and whether its request declares exactly that tool's
/// arguments (`embed_fields` `None`: the request type is the MCP tool's own, which accepts
/// unknown keys).
#[cfg(feature = "embed")]
fn mcp_counterpart(
    label: &str,
    embed_fields: Option<&[String]>,
) -> (Option<&'static str>, Option<&'static [&'static str]>) {
    let tool = EMBED_REQUEST_TOOLS
        .iter()
        .find(|(name, _)| *name == label)
        .and_then(|(_, tool)| *tool);
    let published = tool.and_then(tool_arguments);
    let same = match (published, embed_fields) {
        (Some(_), None) => published,
        (Some(published), Some(fields)) => {
            let mut fields: Vec<&str> = fields.iter().map(String::as_str).collect();
            let mut names = published.to_vec();
            fields.sort_unstable();
            names.sort_unstable();
            (fields == names).then_some(published)
        }
        (None, _) => None,
    };
    (tool, same)
}

/// The refusal an embed request with `unknown` keys gets. `label` is its wire label
/// ([`EMBED_REQUEST_TOOLS`]); `embed_fields` the fields its request type declares, when
/// they can be read. A request that answers for an MCP tool and declares exactly its
/// arguments gets the MCP lane's text, byte for byte. One that declares other arguments
/// (`field_set_differences_are_pinned`) is named by the MCP tool but lists its OWN fields,
/// in their declaration order, so a caller following the list is not refused again. A
/// request answering for no MCP tool is named by its label, with its own fields.
#[cfg(feature = "embed")]
pub(crate) fn embed_unknown_arguments_refusal(
    label: &str,
    unknown: &UnknownArguments,
    embed_fields: Option<&[String]>,
) -> String {
    let (tool, same) = mcp_counterpart(label, embed_fields);
    let valid: Vec<String> = match (same, embed_fields) {
        (Some(published), _) => published.iter().map(|s| (*s).to_string()).collect(),
        (None, fields) => fields.map(<[String]>::to_vec).unwrap_or_default(),
    };
    unknown_arguments_refusal(tool.unwrap_or(label), unknown, &valid)
}

/// The text an embed request whose arguments fail to decode (a wrong type, a missing
/// field) gets: the MCP lane's, byte for byte, where the request declares the MCP tool's
/// arguments. The required-fields line is the MCP tool's, and is left out wherever it
/// would not hold for this request: a request with other arguments, or a missing field
/// the MCP tool does not require.
#[cfg(feature = "embed")]
pub(crate) fn embed_decode_failure_text(
    label: &str,
    serde_message: &str,
    embed_fields: Option<&[String]>,
) -> String {
    let (tool, same) = mcp_counterpart(label, embed_fields);
    let required = match (tool, same) {
        (Some(tool), Some(_)) => tool_required(tool),
        _ => &[],
    };
    let missing = element_prefixes(serde_message)
        .1
        .strip_prefix("missing field `")
        .and_then(|rest| rest.split('`').next());
    let required = match missing {
        Some(field) if !required.contains(&field) => &[],
        _ => required,
    };
    decode_failure_text(tool.unwrap_or(label), serde_message, required)
}

/// The required top-level arguments of MCP tool `tool`, in the order the MCP lane prints
/// them (its schema's `required`).
#[cfg(feature = "embed")]
fn tool_required(tool: &str) -> &'static [&'static str] {
    TOOL_REQUIRED
        .iter()
        .find(|(name, _)| *name == tool)
        .map_or(&[], |(_, required)| *required)
}

/// Every MCP tool with required arguments, and those arguments as its published schema
/// lists them. Pinned to the live schemas by `tool_required_match_the_published_schemas`.
#[cfg(any(feature = "embed", test))]
pub(crate) static TOOL_REQUIRED: &[(&str, &[&str])] = &[
    ("analyze_file_impact", &["path"]),
    ("ask", &["query"]),
    ("batch_edit", &["edits"]),
    ("batch_insert", &["content", "position", "targets"]),
    ("batch_rename", &["path", "name", "new_name"]),
    (
        "curate_knowledge",
        &[
            "actions",
            "if_source_review_hash",
            "if_manifest_digest",
            "if_policy_digest",
            "project",
        ],
    ),
    ("delete_symbol", &["path", "name"]),
    ("edit_plan", &["target"]),
    (
        "edit_within_symbol",
        &["path", "name", "old_text", "new_text"],
    ),
    ("explore", &["query"]),
    ("find_dependents", &["path"]),
    ("find_references", &["name"]),
    ("get_file_content", &["path"]),
    ("get_file_context", &["path"]),
    ("get_symbol_context", &["name"]),
    ("index_folder", &["path"]),
    ("insert_symbol", &["path", "name", "content"]),
    ("inspect_match", &["path", "line"]),
    ("replace_symbol_body", &["path", "name", "new_body"]),
    ("review_knowledge", &["mode"]),
    ("search_knowledge", &["query"]),
    ("secret_remediate", &["scope", "finding_ids", "action"]),
    ("symforge_edit", &["path"]),
    ("symforge_retrieve", &["hash"]),
    ("validate_file_syntax", &["path"]),
];

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[derive(serde::Deserialize, Debug)]
    #[serde(deny_unknown_fields)]
    #[allow(dead_code)]
    struct Inner {
        a: Option<u32>,
    }

    #[derive(serde::Deserialize, Debug)]
    #[serde(deny_unknown_fields)]
    #[allow(dead_code)]
    struct Closed {
        name: Option<String>,
        inner: Option<Inner>,
        items: Option<Vec<Inner>>,
    }

    fn unknown_of(value: Value) -> UnknownArguments {
        match decode_strict::<Closed>(value) {
            Err(StrictError::Unknown(unknown)) => unknown,
            other => panic!("not an unknown-key refusal: {other:?}"),
        }
    }

    fn keys_of(value: Value) -> Vec<String> {
        unknown_of(value).paths()
    }

    /// A closed struct reports one key per decode; every key is found, in document order
    /// (`serde_json::Map` orders members by name in this crate).
    #[test]
    fn every_unknown_key_of_a_closed_struct_is_found_in_order() {
        assert_eq!(
            keys_of(json!({ "zz": 1, "name": "n", "yy": 2 })),
            ["yy", "zz"]
        );
        assert_eq!(
            keys_of(json!({ "items": [{ "zz": 1, "yy": 2 }], "top": 3 })),
            ["items.0.yy", "items.0.zz", "top"]
        );
    }

    /// A key inside a nested object is named by its path, not as a top-level argument.
    #[test]
    fn a_key_in_a_nested_object_is_named_by_its_path() {
        assert_eq!(keys_of(json!({ "inner": { "zz": 1 } })), ["inner.zz"]);
    }

    /// A key containing `.` is one segment: it is removed and the search goes on.
    #[test]
    fn a_key_with_a_dot_does_not_stop_the_search() {
        assert_eq!(keys_of(json!({ "a.b": 1, "zz": 2 })), ["a.b", "zz"]);
    }

    /// A type error whose VALUE contains the words is not an unknown key.
    #[test]
    fn a_value_that_reads_like_an_unknown_field_is_not_a_key() {
        assert!(matches!(
            decode_strict::<Closed>(json!({ "inner": { "a": "unknown field `x`" } })),
            Err(StrictError::Decode(_))
        ));
    }

    /// Past the round cap the refusal says more keys exist than it lists.
    #[test]
    fn the_key_list_stops_at_its_cap() {
        let many: Map<String, Value> = (0..40).map(|i| (format!("k{i:02}"), json!(i))).collect();
        let unknown = unknown_of(Value::Object(many));
        let text = unknown_arguments_refusal("t", &unknown, &["name".to_string()]);
        assert_eq!(
            text,
            "invalid_request: t does not accept \"k00\", \"k01\", \"k02\", \"k03\", \"k04\", \
             \"k05\", \"k06\", \"k07\" (+24 or more). Valid arguments: name."
        );
    }

    #[test]
    fn values_are_redacted_from_decode_failures() {
        assert_eq!(
            decode_failure_text(
                "t",
                "invalid type: integer `4242`, expected a string",
                &["path"]
            ),
            "failed to deserialize parameters: invalid type: integer, expected a string\n\
             Required fields for `t`: path."
        );
        assert_eq!(
            decode_failure_text(
                "t",
                "invalid type: string \"se\\\"cret\", expected u32",
                &[]
            ),
            "failed to deserialize parameters: invalid type: string, expected u32"
        );
        assert_eq!(
            decode_failure_text("t", "unknown variant `SECRET`, expected `a` or `b`", &[]),
            "failed to deserialize parameters: unknown variant, expected `a` or `b`"
        );
        assert_eq!(
            decode_failure_text("t", "missing field `path`", &["path"]),
            "failed to deserialize parameters: missing field `path`\nRequired fields for `t`: path."
        );
    }

    /// Echoed values are cut; the cut keeps the start.
    #[test]
    fn echoed_names_are_cut() {
        let long = "x".repeat(500);
        assert!(capped_key(&long).chars().count() < KEY_ECHO_LIMIT + 8);
        assert_eq!(capped_key("zz"), "zz");
        let unknown = UnknownArguments {
            keys: vec![UnknownKey::single(&long)],
            incomplete: false,
        };
        let text = unknown_arguments_refusal("t", &unknown, &["a".to_string()]);
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

    /// The required-fields table equals the published schemas' `required` lists.
    #[cfg(feature = "server")]
    #[test]
    fn tool_required_match_the_published_schemas() {
        let mut published: Vec<(String, Vec<String>)> =
            crate::protocol::SymForgeServer::tool_definitions()
                .into_iter()
                .filter_map(|tool| {
                    let required: Vec<String> = tool
                        .input_schema
                        .get("required")
                        .and_then(Value::as_array)
                        .map(|names| {
                            names
                                .iter()
                                .filter_map(Value::as_str)
                                .map(str::to_string)
                                .collect()
                        })
                        .unwrap_or_default();
                    (!required.is_empty()).then(|| (tool.name.to_string(), required))
                })
                .collect();
        published.sort();
        let mut table: Vec<(String, Vec<String>)> = TOOL_REQUIRED
            .iter()
            .map(|(tool, required)| {
                (
                    (*tool).to_string(),
                    required.iter().map(|a| (*a).to_string()).collect(),
                )
            })
            .collect();
        table.sort();
        let literal = published
            .iter()
            .map(|(tool, required)| {
                let quoted: Vec<String> = required.iter().map(|a| format!("\"{a}\"")).collect();
                format!("    (\"{tool}\", &[{}]),", quoted.join(", "))
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            table, published,
            "TOOL_REQUIRED drifted from the published schemas; the table is:\n{literal}"
        );
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
