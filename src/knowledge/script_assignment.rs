//! Exact reference-only assignments inside parsed inline JavaScript.
use std::ops::Range;

use crate::domain::LanguageId;

#[derive(Default)]
pub(super) struct ProvenAssignments {
    assignments: Vec<Assignment>,
}

struct Assignment {
    key: Range<usize>,
    rhs: Range<usize>,
    max_capture_end: usize,
}

impl ProvenAssignments {
    pub(super) fn new(bytes: &[u8]) -> Self {
        let mut proof = Self::default();
        let Ok(source) = std::str::from_utf8(bytes) else {
            return proof;
        };
        let Ok(html) = crate::parsing::parse_source_tree(source, &LanguageId::Html, false) else {
            return proof;
        };
        if html.root_node().has_error() {
            return proof;
        }
        let mut pending = vec![html.root_node()];
        let mut budget = 100_000_usize;
        let mut scripts = 0;
        while let Some(node) = pending.pop() {
            if budget == 0 {
                break;
            }
            budget -= 1;
            if node.kind() == "script_element" {
                scripts += 1;
                if scripts > 32 {
                    break;
                }
                if node.has_error() || !is_inline_javascript(node, bytes) {
                    continue;
                }
                let mut cursor = node.walk();
                let Some(raw) = node
                    .named_children(&mut cursor)
                    .find(|child| child.kind() == "raw_text")
                else {
                    continue;
                };
                let Some(script) = source.get(raw.byte_range()) else {
                    continue;
                };
                let Ok(tree) =
                    crate::parsing::parse_source_tree(script, &LanguageId::JavaScript, false)
                else {
                    continue;
                };
                if tree.root_node().has_error() {
                    continue;
                }
                let mut nodes = vec![tree.root_node()];
                while let Some(node) = nodes.pop() {
                    if budget == 0 {
                        break;
                    }
                    budget -= 1;
                    if matches!(node.kind(), "variable_declarator" | "assignment_expression") {
                        let key = node
                            .child_by_field_name("name")
                            .or_else(|| node.child_by_field_name("left"));
                        let value = node
                            .child_by_field_name("value")
                            .or_else(|| node.child_by_field_name("right"));
                        if let (Some(key), Some(value)) = (key, value)
                            && key.kind() == "identifier"
                            && reference_only(value, &mut budget)
                        {
                            let rhs = value.start_byte() + raw.start_byte()
                                ..value.end_byte() + raw.start_byte();
                            let Some(tail) = bytes.get(rhs.end..raw.end_byte()) else {
                                continue;
                            };
                            let max_capture_end = rhs.end
                                + tail
                                    .iter()
                                    .take_while(|byte| {
                                        matches!(byte, b')' | b']' | b'}' | b',' | b';')
                                    })
                                    .count();
                            proof.assignments.push(Assignment {
                                key: key.start_byte() + raw.start_byte()
                                    ..key.end_byte() + raw.start_byte(),
                                rhs,
                                max_capture_end,
                            });
                        }
                    }
                    let mut cursor = node.walk();
                    nodes.extend(node.named_children(&mut cursor));
                }
                continue;
            }
            let mut cursor = node.walk();
            pending.extend(node.named_children(&mut cursor));
        }
        proof
            .assignments
            .sort_unstable_by_key(|assignment| assignment.rhs.start);
        proof
    }

    pub(super) fn contains(&self, key: usize, value: usize, value_end: usize) -> bool {
        self.assignments
            .binary_search_by_key(&value, |assignment| assignment.rhs.start)
            .ok()
            .is_some_and(|index| {
                let assignment = &self.assignments[index];
                assignment.key.contains(&key)
                    && value_end > value
                    && value_end <= assignment.max_capture_end
            })
    }
}

fn is_inline_javascript(node: tree_sitter::Node<'_>, bytes: &[u8]) -> bool {
    let mut cursor = node.walk();
    let Some(tag) = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == "start_tag")
    else {
        return false;
    };
    let mut cursor = tag.walk();
    let mut found_type = false;
    for attribute in tag
        .named_children(&mut cursor)
        .filter(|child| child.kind() == "attribute")
    {
        let mut cursor = attribute.walk();
        let Some(name) = attribute
            .named_children(&mut cursor)
            .find(|child| child.kind() == "attribute_name")
        else {
            return false;
        };
        let Some(name) = bytes.get(name.byte_range()) else {
            return false;
        };
        if name.eq_ignore_ascii_case(b"src") || name.eq_ignore_ascii_case(b"language") {
            return false;
        }
        if !name.eq_ignore_ascii_case(b"type") {
            continue;
        }
        if found_type {
            return false;
        }
        found_type = true;
        let mut cursor = attribute.walk();
        let Some(value) = attribute
            .named_children(&mut cursor)
            .find(|child| matches!(child.kind(), "attribute_value" | "quoted_attribute_value"))
        else {
            return false;
        };
        let Some(mut value) = bytes.get(value.byte_range()) else {
            return false;
        };
        if value
            .first()
            .is_some_and(|byte| matches!(byte, b'\'' | b'"'))
        {
            if value.len() < 2 || value.last() != value.first() {
                return false;
            }
            value = &value[1..value.len() - 1];
        }
        let value = value.trim_ascii();
        if ![
            b"".as_slice(),
            b"module",
            b"text/javascript",
            b"application/javascript",
            b"text/ecmascript",
            b"application/ecmascript",
        ]
        .iter()
        .any(|allowed| value.eq_ignore_ascii_case(allowed))
        {
            return false;
        }
    }
    true
}

fn reference_only(value: tree_sitter::Node<'_>, budget: &mut usize) -> bool {
    let mut pending = vec![value];
    let mut remaining = 512;
    while let Some(node) = pending.pop() {
        if remaining == 0 || *budget == 0 {
            return false;
        }
        remaining -= 1;
        *budget -= 1;
        if !matches!(
            node.kind(),
            "identifier"
                | "property_identifier"
                | "member_expression"
                | "call_expression"
                | "arguments"
                | "parenthesized_expression"
                | "subscript_expression"
                | "optional_chain"
        ) {
            return false;
        }
        let mut cursor = node.walk();
        pending.extend(node.named_children(&mut cursor));
    }
    true
}

#[cfg(test)]
mod tests {
    use crate::knowledge::{SecretScan, SecretSpansScan, scan_secret_bytes, scan_secret_spans};

    fn fixture_body(body: &str) -> String {
        body.replace("FIELD", "token").replace("OTHER", "password")
    }

    fn is_clean(template: &str) {
        let body = fixture_body(template);
        assert!(matches!(
            scan_secret_bytes("fixture.html", body.as_bytes()),
            SecretScan::Clean
        ));
        assert!(
            matches!(scan_secret_spans("fixture.html", body.as_bytes()), SecretSpansScan::Spans(ref spans) if spans.is_empty())
        );
    }

    fn is_sensitive(template: &str) {
        let body = fixture_body(template);
        assert!(matches!(
            scan_secret_bytes("fixture.html", body.as_bytes()),
            SecretScan::Sensitive { .. }
        ));
        assert!(
            matches!(scan_secret_spans("fixture.html", body.as_bytes()), SecretSpansScan::Spans(ref spans) if !spans.is_empty())
        );
    }

    #[test]
    fn exact_script_assignments_ignore_unrelated_following_literals() {
        is_clean("<script>function f(){const FIELD=(incoming.member());} </script>");
        for tag in [
            "<script>",
            "<script type=\"module\">",
            "<script type=\"application/javascript\">",
        ] {
            is_clean(&format!(
                "{tag}const FIELD = incoming.member(); const OTHER = cached.member(); render(\"ordinary following literal\");</script>"
            ));
            is_clean(&format!(
                "{tag}\nconst FIELD = incoming.member();\nrender(\"ordinary following literal\");\n</script>"
            ));
            is_clean(&format!(
                "{tag}\nFIELD = incoming.member();\nrender(\"ordinary following literal\");\n</script>"
            ));
        }
    }

    #[test]
    fn literal_opaque_and_ambiguous_scripts_remain_sensitive() {
        let payload = "Q7m9V2n6A4z8L3p5".repeat(3);
        for body in [
            format!("<script>const FIELD = \"{payload}\";</script>"),
            format!("<script>const FIELD = resolveIncomingValue(\"{payload}\");</script>"),
            format!("<script>const FIELD = incoming || \"{payload}\";</script>"),
            format!("<script>const FIELD = {{long_field_name: \"{payload}\"}};</script>"),
            format!("<script>const FIELD = incoming; const OTHER = \"{payload}\";</script>"),
            format!("<script>const FIELD = incoming, OTHER = \"{payload}\";</script>"),
            format!("<script>FIELD=references;OTHER=\"{payload}\";</script>"),
            format!("<script>const FIELD=references,OTHER=\"{payload}\";</script>"),
            format!("<script>function f(){{FIELD=references;}}OTHER=\"{payload}\";</script>"),
            format!("<script>const FIELD = incoming; /* -----BEGIN {} KEY----- */</script>", "PRIVATE"),
            "<script type=\"application/json\">const FIELD = incoming.member();</script>".to_string(),
            "<script type=\"custom/type\">const FIELD = incoming.member();</script>".to_string(),
            "<script src=\"external.js\">const FIELD = incoming.member();</script>".to_string(),
            "<script language=\"other\">const FIELD = incoming.member();</script>".to_string(),
            "<script type=\"module\" type=\"application/json\">const FIELD = incoming.member();</script>".to_string(),
            "<script>const FIELD = incoming.member(); invalid(</script>".to_string(),
            "<p>FIELD = incoming.member()</p>".to_string(),
            "<script>object.FIELD = incoming.member();</script>".to_string(),
        ] { is_sensitive(&body); }
    }
}
