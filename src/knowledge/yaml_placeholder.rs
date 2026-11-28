use std::cell::OnceCell;
use std::ops::Range;

use serde_saphyr::granit_parser::{Event, Options, Parser, ScalarStyle, StructureStyle};

enum Frame {
    Mapping { flow: bool, value: bool },
    Sequence,
}

fn take_mapping_value(stack: &mut [Frame]) -> bool {
    if let Some(Frame::Mapping { flow, value }) = stack.last_mut() {
        let owned = *flow && *value;
        *value = !*value;
        owned
    } else {
        false
    }
}

fn mapping_values(bytes: &[u8]) -> Vec<Range<usize>> {
    let Ok(source) = std::str::from_utf8(bytes) else {
        return Vec::new();
    };
    let mut options = Options::default();
    options.strict_indentation = false;
    let mut parser = Parser::new_from_str_with_options(source, options);
    let mut stack = Vec::new();
    let mut markers = Vec::new();
    let mut complete = false;
    while let Some(event) = parser.next_event() {
        let Ok((event, span)) = event else {
            return Vec::new();
        };
        match event {
            Event::MappingStart(style, anchor, tag) => {
                take_mapping_value(&mut stack);
                stack.push(Frame::Mapping {
                    flow: matches!(style, StructureStyle::Flow) && anchor == 0 && tag.is_none(),
                    value: false,
                });
            }
            Event::SequenceStart(..) => {
                take_mapping_value(&mut stack);
                stack.push(Frame::Sequence);
            }
            Event::MappingEnd | Event::SequenceEnd => {
                let _ = stack.pop();
            }
            Event::Scalar(_, style, anchor, tag) => {
                let owned = take_mapping_value(&mut stack);
                if owned
                    && matches!(style, ScalarStyle::Plain)
                    && anchor == 0
                    && tag.is_none()
                    && let Some(range) = span.byte_range()
                    && bytes.get(range.clone()).is_some_and(|value| {
                        super::has_placeholder_prefix(value) && super::is_placeholder_name(value)
                    })
                {
                    markers.push(range);
                }
            }
            Event::Alias(_) | Event::UnresolvedAlias(_) => {
                take_mapping_value(&mut stack);
            }
            Event::StreamEnd => complete = true,
            _ => {}
        }
        if stack.len() > 128 {
            return Vec::new();
        }
    }
    if !complete || !stack.is_empty() {
        return Vec::new();
    }
    markers.sort_by_key(|range| range.start);
    markers
}

pub(super) fn placeholder_ends_at_mapping_value(
    path: &str,
    bytes: &[u8],
    value_start: usize,
    value_end: usize,
    markers: &OnceCell<Vec<Range<usize>>>,
) -> bool {
    if crate::domain::LanguageId::from_path(path) != Some(crate::domain::LanguageId::Yaml) {
        return false;
    }
    let Some(value) = bytes.get(value_start..value_end) else {
        return false;
    };
    let marker_end = value
        .iter()
        .position(|byte| !byte.is_ascii_alphanumeric() && !matches!(byte, b'_' | b'-'))
        .unwrap_or(value.len());
    let marker = &value[..marker_end];
    if !super::has_placeholder_prefix(marker)
        || !super::is_placeholder_name(marker)
        || !value[marker_end..]
            .iter()
            .all(|byte| matches!(byte, b'}' | b']' | b','))
    {
        return false;
    }
    // Never discard a capture that has also swallowed the next mapping key.
    let markers = markers.get_or_init(|| mapping_values(bytes));
    markers
        .binary_search_by_key(&value_start, |range| range.start)
        .is_ok_and(|at| markers[at].end == value_start + marker_end)
}

#[cfg(test)]
mod tests {
    use super::super::{SecretScan, SecretSpansScan, scan_secret_bytes, scan_secret_spans};

    fn check(source: &str, clean: bool) {
        assert_eq!(
            matches!(
                scan_secret_bytes("config.yml", source.as_bytes()),
                SecretScan::Clean
            ),
            clean
        );
        assert!(
            matches!(scan_secret_spans("config.yml", source.as_bytes()), SecretSpansScan::Spans(spans) if spans.is_empty() == clean)
        );
    }

    #[test]
    fn flow_mapping_marker_uses_the_scalar_boundary() {
        let key = "api_key";
        for source in [
            format!("{{{key}: your_marker}}"),
            format!("{{{key}: your_marker, enabled: true}}"),
            format!("{{{key}: your_marker, label: \"ordinary label\"}}"),
            format!("outer: {{{key}: your-marker}}"),
            format!("{{label: café, {key}: your_marker}}"),
        ] {
            check(&source, true);
        }
    }

    #[test]
    fn other_values_and_ambiguous_yaml_stay_guarded() {
        let key = "api_key";
        let other = "password";
        let canary = ["Opaque", "Fixture", "Canary"].concat();
        for source in [
            format!("{{{key}: your_marker, {other}: \"{canary}\"}}"),
            format!("{{{key}: your_marker,{other}: \"{canary}\"}}"),
            format!("{{{key}: [\"your_marker\", \"{canary}\"]}}"),
            format!("{{{key}: your_marker+{canary}}}"),
            format!("{{{key}: your_marker, label: \"{canary}\""),
            format!("{{label: \"{key}=your_marker}}\"}}"),
        ] {
            check(&source, false);
        }
    }
}
