//! Shared wire coercions retained for protocol compatibility.
use serde::{Deserialize, Deserializer};

/// Deserialize a `u32` from either a JSON number or a stringified number like `"5"`.
pub(crate) fn lenient_u32<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<u32>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NumOrStr {
        Num(u32),
        Str(String),
        Null,
    }
    match NumOrStr::deserialize(deserializer)? {
        NumOrStr::Num(n) => Ok(Some(n)),
        NumOrStr::Str(s) if s.is_empty() => Ok(None),
        NumOrStr::Str(s) => s.parse::<u32>().map(Some).map_err(serde::de::Error::custom),
        NumOrStr::Null => Ok(None),
    }
}

/// Deserialize a `u64` from either a JSON number or a stringified number.
pub(crate) fn lenient_u64<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<u64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NumOrStr {
        Num(u64),
        Str(String),
        Null,
    }
    match NumOrStr::deserialize(deserializer)? {
        NumOrStr::Num(n) => Ok(Some(n)),
        NumOrStr::Str(s) if s.is_empty() => Ok(None),
        NumOrStr::Str(s) => s.parse::<u64>().map(Some).map_err(serde::de::Error::custom),
        NumOrStr::Null => Ok(None),
    }
}

/// Deserialize a `bool` from either a JSON boolean or a stringified boolean like `"true"`.
pub(crate) fn lenient_bool<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<bool>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum BoolOrStr {
        Bool(bool),
        Str(String),
        Null,
    }
    match BoolOrStr::deserialize(deserializer)? {
        BoolOrStr::Bool(b) => Ok(Some(b)),
        BoolOrStr::Str(s) => match s.as_str() {
            "true" | "1" => Ok(Some(true)),
            "false" | "0" => Ok(Some(false)),
            "" => Ok(None),
            _ => Err(serde::de::Error::custom(format!(
                "expected boolean or \"true\"/\"false\", got \"{s}\""
            ))),
        },
        BoolOrStr::Null => Ok(None),
    }
}

/// Leniently deserialize an `Option<Vec<T>>` — accepts a native JSON array, a
/// stringified JSON array (as sent by some MCP clients like Kilo Code), or a
/// native array of stringified JSON objects (as sent by Codex).
pub(crate) fn lenient_option_vec<'de, D, T>(deserializer: D) -> Result<Option<Vec<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum VecOrStr {
        Vec(Vec<serde_json::Value>),
        Str(String),
        Null,
    }
    match VecOrStr::deserialize(deserializer)? {
        VecOrStr::Vec(values) => {
            let result: Result<Vec<T>, _> = values
                .into_iter()
                .enumerate()
                .map(|(i, v)| match serde_json::from_value::<T>(v.clone()) {
                    Ok(item) => Ok(item),
                    Err(direct_err) => {
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
                })
                .collect();
            result.map(Some)
        }
        VecOrStr::Str(s) if s.is_empty() || s == "null" => Ok(None),
        VecOrStr::Str(s) => serde_json::from_str::<Vec<T>>(&s)
            .map(Some)
            .map_err(serde::de::Error::custom),
        VecOrStr::Null => Ok(None),
    }
}
