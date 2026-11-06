//! Untyped YAML document model for the YAML and CI-YAML extractors.
//!
//! `serde-saphyr` is a pure-Rust (no `unsafe-libyaml`) serde deserializer but
//! ships no dynamic `Value`, so this module supplies the small subset the
//! extractors walk: insertion-ordered mappings keyed by `Value`, sequences,
//! scalars, and an opaque `Tagged` marker for application tags (`!reference`,
//! `!Ref`, ...) whose contents the extractors skip, as they did with
//! `serde_yaml_ng::Value::Tagged`.

use std::fmt;

use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_saphyr::{DuplicateKeyPolicy, MergeKeyPolicy, NonFiniteFloatPolicy, Options, Tagged};

/// Prefix of every resolved YAML core-schema tag (`!!str`, `!!int`, ...).
const CORE_TAG_PREFIX: &str = "tag:yaml.org,2002:";

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Value {
    Null,
    Bool(bool),
    Number(serde_json::Number),
    String(String),
    Sequence(Vec<Value>),
    Mapping(Mapping),
    /// A node carrying an application tag; its content is deliberately dropped.
    Tagged,
}

/// Insertion-ordered YAML mapping. Order matters: the extractors scan byte
/// ranges forward in iteration order.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Mapping(Vec<(Value, Value)>);

impl Mapping {
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&Value, &Value)> {
        self.0.iter().map(|(k, v)| (k, v))
    }

    /// Look up a scalar string key.
    // ponytail: linear scan; mappings here are config-sized.
    pub(crate) fn get(&self, key: &str) -> Option<&Value> {
        self.0.iter().find_map(|(k, v)| match k {
            Value::String(s) if s == key => Some(v),
            _ => None,
        })
    }
}

/// Parse one YAML document with options matching the previous `serde_yaml_ng`
/// semantics (YAML 1.2 core booleans, no merge-key expansion, duplicate keys
/// rejected) while keeping libyaml-style lenient flow indentation
/// (SF-STRESS-016, `strict_indentation: false`).
pub(crate) fn from_slice(content: &[u8]) -> Result<Value, serde_saphyr::Error> {
    serde_saphyr::from_slice_with_options(content, options())
}

fn options() -> Options {
    let mut options = Options::default();
    options.strict_indentation = false;
    // `on:` must stay a string key (GitHub Actions), not YAML 1.1 `true`.
    options.strict_booleans = true;
    options.merge_keys = MergeKeyPolicy::AsOrdinary;
    options.duplicate_keys = DuplicateKeyPolicy::Error;
    options.leading_zero_numbers_as_strings = true;
    options.underscored_numbers_as_strings = true;
    options.non_finite_float_policy = NonFiniteFloatPolicy::AsString;
    options.with_snippet = false;
    if let Some(budget) = options.budget.as_mut() {
        // Lockfiles routinely exceed the 250k-node default; serde_yaml_ng had
        // no node cap and a 128-level recursion limit.
        budget.max_nodes = usize::MAX;
        budget.max_events = usize::MAX;
        budget.max_depth = 128;
    }
    options
}

impl<'de> Deserialize<'de> for Value {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let Tagged(Untagged(value), tag) = Tagged::<Untagged>::deserialize(deserializer)?;
        match tag {
            Some(tag) if tag != "!" && !tag.starts_with(CORE_TAG_PREFIX) => Ok(Value::Tagged),
            _ => Ok(value),
        }
    }
}

/// A node deserialized without tag inspection (children still use `Value`).
struct Untagged(Value);

impl<'de> Deserialize<'de> for Untagged {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(ValueVisitor).map(Untagged)
    }
}

struct ValueVisitor;

impl<'de> Visitor<'de> for ValueVisitor {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("any YAML value")
    }

    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_none<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        Value::deserialize(deserializer)
    }

    fn visit_bool<E>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }

    fn visit_i64<E>(self, v: i64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }

    fn visit_u64<E>(self, v: u64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }

    fn visit_f64<E>(self, v: f64) -> Result<Value, E> {
        // Non-finite floats arrive as `.nan`/`.inf` strings (AsString policy).
        Ok(serde_json::Number::from_f64(v).map_or(Value::Null, Value::Number))
    }

    fn visit_str<E>(self, v: &str) -> Result<Value, E> {
        Ok(Value::String(v.to_owned()))
    }

    fn visit_string<E>(self, v: String) -> Result<Value, E> {
        Ok(Value::String(v))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut items = Vec::with_capacity(seq.size_hint().unwrap_or(0));
        while let Some(item) = seq.next_element::<Value>()? {
            items.push(item);
        }
        Ok(Value::Sequence(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut entries = Vec::with_capacity(map.size_hint().unwrap_or(0));
        while let Some((Untagged(key), value)) = map.next_entry::<Untagged, Value>()? {
            entries.push((key, value));
        }
        Ok(Value::Mapping(Mapping(entries)))
    }

    fn visit_enum<A: de::EnumAccess<'de>>(self, data: A) -> Result<Value, A::Error> {
        // Safety net: untyped `deserialize_any` never surfaces tags as enums
        // (tags arrive via `Tagged` above), but drop any enum-shaped node opaquely.
        let (de::IgnoredAny, variant) = data.variant()?;
        de::VariantAccess::newtype_variant::<de::IgnoredAny>(variant)?;
        Ok(Value::Tagged)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn top(content: &str) -> Mapping {
        match from_slice(content.as_bytes()).expect("parses") {
            Value::Mapping(map) => map,
            other => panic!("expected mapping, got {other:?}"),
        }
    }

    #[test]
    fn on_key_stays_a_string_and_order_is_preserved() {
        let map = top("on: push\njobs: {}\nb: 1\na: 2\n");
        let keys: Vec<_> = map
            .iter()
            .filter_map(|(k, _)| match k {
                Value::String(s) => Some(s.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(keys, ["on", "jobs", "b", "a"]);
    }

    #[test]
    fn numbers_render_like_serde_yaml_ng() {
        let map = top("a: 1.0\nb: 3.10\nc: -7\nd: 0755\ne: .nan\n");
        let render = |k| match map.get(k) {
            Some(Value::Number(n)) => n.to_string(),
            Some(Value::String(s)) => s.clone(),
            other => panic!("{k}: {other:?}"),
        };
        assert_eq!(render("a"), "1.0");
        assert_eq!(render("b"), "3.1");
        assert_eq!(render("c"), "-7");
        assert_eq!(render("d"), "0755");
        assert_eq!(render("e"), ".nan");
    }

    #[test]
    fn application_tags_are_opaque_and_merge_keys_are_ordinary() {
        let map = top("a: !reference [x, y]\nb: !!str 5\nbase: &b {k: 1}\nc:\n  <<: *b\n");
        assert_eq!(map.get("a"), Some(&Value::Tagged));
        assert_eq!(map.get("b"), Some(&Value::String("5".into())));
        let Some(Value::Mapping(c)) = map.get("c") else {
            panic!("c is a mapping");
        };
        assert!(c.get("<<").is_some(), "merge key kept as ordinary key");
    }

    #[test]
    fn multi_document_input_is_rejected_like_serde_yaml_ng() {
        assert!(from_slice(b"a: 1
---
b: 2
").is_err());
    }

    #[test]
    fn comment_only_document_is_null() {
        assert_eq!(from_slice(b"# just a comment\n").expect("parses"), Value::Null);
    }
}
