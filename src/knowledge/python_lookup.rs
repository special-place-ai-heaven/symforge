//! Lexical evidence for a field-name literal, never proof of runtime module origin.

use std::collections::HashMap;
use std::ops::Range;

use tree_sitter::Node;

use crate::domain::LanguageId;

#[derive(Default)]
pub(super) struct ProvenLookups {
    assignments: Vec<(Range<usize>, usize)>,
}

struct ImportedRequest {
    name: Range<usize>,
    end: usize,
    blocked: bool,
}

impl ProvenLookups {
    pub(super) fn new(path: &str, bytes: &[u8]) -> Self {
        if LanguageId::from_path(path) != Some(LanguageId::Python)
            || !super::classify_scan_path(path).relaxed
            || super::exceeds_scan_limit(bytes.len())
        {
            return Self::default();
        }
        let Ok(source) = std::str::from_utf8(bytes) else {
            return Self::default();
        };
        let Ok(tree) = crate::parsing::parse_source_tree(source, &LanguageId::Python, false) else {
            return Self::default();
        };
        let root = tree.root_node();
        if root.has_error() {
            return Self::default();
        }
        let mut imports = HashMap::new();
        for statement in root.named_children(&mut root.walk()) {
            if statement.kind() != "import_from_statement"
                || statement
                    .child_by_field_name("module_name")
                    .and_then(|module| module.utf8_text(bytes).ok())
                    != Some("flask")
            {
                continue;
            }
            for name in statement.children_by_field_name("name", &mut statement.walk()) {
                let (imported, local) = if name.kind() == "aliased_import" {
                    (
                        name.child_by_field_name("name"),
                        name.child_by_field_name("alias"),
                    )
                } else {
                    (Some(name), name.named_child(0))
                };
                if imported.and_then(|name| name.utf8_text(bytes).ok()) != Some("request") {
                    continue;
                }
                if let Some(local) = local.filter(|local| local.kind() == "identifier")
                    && let Ok(name) = local.utf8_text(bytes)
                {
                    imports
                        .entry(name)
                        .and_modify(|import: &mut ImportedRequest| import.blocked = true)
                        .or_insert(ImportedRequest {
                            name: local.byte_range(),
                            end: statement.end_byte(),
                            blocked: false,
                        });
                }
            }
        }
        if imports.is_empty() {
            return Self::default();
        }

        let mut candidates = Vec::new();
        let mut receivers = Vec::new();
        let mut writes = Vec::new();
        let mut cursor = root.walk();
        loop {
            let node = cursor.node();
            if node.kind() == "wildcard_import" {
                return Self::default();
            }
            if node.kind() == "identifier"
                && let Ok(name) = node.utf8_text(bytes)
                && let Some(import) = imports.get_mut(name)
                && node.byte_range() != import.name
            {
                if node.parent().is_some_and(|parent| {
                    parent.kind() == "attribute"
                        && parent.child_by_field_name("object") == Some(node)
                }) {
                    receivers.push((node.start_byte(), name));
                } else {
                    // An opaque use can alias or replace the receiver; reject file-wide.
                    import.blocked = true;
                }
            }
            if let Some(target) = writable_target(node) {
                writes.push(target.byte_range());
            }
            if node.kind() == "assignment"
                && let Some(candidate) = lookup_assignment(node, bytes)
            {
                candidates.push(candidate);
            }
            if cursor.goto_first_child() {
                continue;
            }
            while !cursor.goto_next_sibling() {
                if !cursor.goto_parent() {
                    writes.sort_unstable_by_key(|range| range.start);
                    let mut max_end = 0;
                    for range in &mut writes {
                        max_end = max_end.max(range.end);
                        range.end = max_end;
                    }
                    for (start, name) in receivers {
                        let index = writes.partition_point(|range| range.start <= start);
                        if index > 0 && start < writes[index - 1].end {
                            imports.get_mut(name).expect("known import").blocked = true;
                        }
                    }
                    let mut assignments: Vec<_> = candidates
                        .into_iter()
                        .filter(|(_, start, name)| {
                            imports
                                .get(name)
                                .is_some_and(|import| !import.blocked && *start >= import.end)
                        })
                        .map(|(key, start, _)| (key, start))
                        .collect();
                    assignments.sort_unstable_by_key(|(_, start)| *start);
                    return Self { assignments };
                }
            }
        }
    }

    pub(super) fn contains(&self, match_start: usize, value_start: usize) -> bool {
        self.assignments
            .binary_search_by_key(&value_start, |(_, start)| *start)
            .is_ok_and(|index| self.assignments[index].0.start == match_start)
    }
}

fn writable_target(node: Node<'_>) -> Option<Node<'_>> {
    match node.kind() {
        "assignment" | "augmented_assignment" | "for_statement" | "for_in_clause" => {
            node.child_by_field_name("left")
        }
        "named_expression" => node.child_by_field_name("name"),
        "as_pattern" => node.child_by_field_name("alias"),
        "delete_statement" => Some(node),
        _ => None,
    }
}

fn lookup_assignment<'source>(
    assignment: Node<'_>,
    bytes: &'source [u8],
) -> Option<(Range<usize>, usize, &'source str)> {
    let key = assignment.child_by_field_name("left")?;
    if key.kind() != "identifier" {
        return None;
    }
    let rhs = assignment.child_by_field_name("right")?;
    let mut lookup = rhs;
    while lookup.kind() == "parenthesized_expression" {
        if lookup.named_child_count() != 1 {
            return None;
        }
        lookup = lookup.named_child(0)?;
    }
    if lookup.kind() != "subscript" || lookup.named_child_count() != 2 {
        return None;
    }
    let receiver = lookup.child_by_field_name("value")?;
    if receiver.kind() != "attribute"
        || receiver.named_child_count() != 2
        || receiver
            .child_by_field_name("attribute")?
            .utf8_text(bytes)
            .ok()?
            != "form"
    {
        return None;
    }
    let request = receiver.child_by_field_name("object")?;
    if request.kind() != "identifier" {
        return None;
    }
    let field = lookup.child_by_field_name("subscript")?;
    let literal = field.utf8_text(bytes).ok()?.as_bytes();
    let key_text = key.utf8_text(bytes).ok()?.as_bytes();
    if field.kind() != "string"
        || literal.len() != key_text.len() + 2
        || !matches!(literal.first(), Some(b'\'' | b'"'))
        || literal.first() != literal.last()
        || &literal[1..literal.len() - 1] != key_text
    {
        return None;
    }
    if !bytes[rhs.end_byte()..]
        .iter()
        .take_while(|byte| **byte != b'\n')
        .all(u8::is_ascii_whitespace)
    {
        return None;
    }
    Some((
        key.byte_range(),
        rhs.start_byte(),
        request.utf8_text(bytes).ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credential_fixture(template: &str) -> String {
        template.replace("FIELD", "password")
    }

    fn proves(path: &str, template: &str) -> bool {
        let source = credential_fixture(template);
        let assignment = format!("{} = ", "password");
        let match_start = source.rfind(&assignment).unwrap();
        let value_start = match_start + assignment.len();
        ProvenLookups::new(path, source.as_bytes()).contains(match_start, value_start)
    }

    #[test]
    fn named_flask_imports_prove_only_matching_plain_field_reads() {
        for source in [
            "from flask import request\nFIELD = request.form['FIELD']\n",
            "from flask import request as req\nFIELD = req.form[\"FIELD\"]\r\n",
            "from flask import (\n    Blueprint,\n    request,\n)\n\ndef login():\n    if request.method == 'POST':\n        FIELD = request.form['FIELD']\n",
            "from flask import request\ndef outer():\n    def login():\n        FIELD = request.form['FIELD']\n",
            "from flask import request\nFIELD = (request.form['FIELD'])\n",
        ] {
            assert!(proves("src/auth.py", source));
        }
    }

    #[test]
    fn unproven_bindings_and_payloads_never_receive_an_exemption() {
        for source in [
            "FIELD = request.form['FIELD']\n",
            "from local import request\nFIELD = request.form['FIELD']\n",
            "from .flask import request\nFIELD = request.form['FIELD']\n",
            "import flask as request\nFIELD = request.form['FIELD']\n",
            "from flask import request\nfrom other import *\nFIELD = request.form['FIELD']\n",
            "from flask import request\nfrom flask import request\nFIELD = request.form['FIELD']\n",
            "from flask import request\nrequest = value\nFIELD = request.form['FIELD']\n",
            "from flask import request\ndef login(request):\n    FIELD = request.form['FIELD']\n",
            "from flask import request\ndef login():\n    FIELD = request.form['FIELD']\n    request = value\n",
            "from flask import request\nfor request in values:\n    FIELD = request.form['FIELD']\n",
            "from flask import request\nwith manager as request:\n    FIELD = request.form['FIELD']\n",
            "from flask import request\ndef request(): pass\nFIELD = request.form['FIELD']\n",
            "from flask import request\nrequest.form = mapping\nFIELD = request.form['FIELD']\n",
            "from flask import request\nother = request\nFIELD = request.form['FIELD']\n",
            "from flask import request\nFIELD = request.form['different']\n",
            "from flask import request\nFIELD = request.form['FIELD', 'extra-literal']\n",
            "from flask import request\nFIELD = local.mapping['FIELD']\n",
            "from flask import request\nFIELD = request.form['FIELD'] or 'fallback-value'\n",
            "from flask import request\nFIELD = request.form.get('FIELD', 'fallback-value')\n",
            "from flask import request\nFIELD = f\"{request.form['FIELD']}\"\n",
            "from flask import request\nFIELD = request.form['FIELD'] # extra literal\n",
            "from flask import request\nFIELD = (\n    request.form['FIELD'] # extra literal\n)\n",
            "from flask import request\nFIELD = ( # extra literal\n    request.form['FIELD'])\n",
            "from flask import request\nFIELD = request.form['FIELD' # extra literal\n]\n",
            "from flask import request\nFIELD = request.form['FIELD'\n",
        ] {
            assert!(!proves("src/auth.py", source));
        }
        let source = "from flask import request\nFIELD = request.form['FIELD']\n";
        for path in ["auth.toml", "config/auth.py", "settings.py", "auth.txt"] {
            assert!(!proves(path, source));
        }
    }

    #[test]
    fn proofs_are_assignment_specific_and_leave_independent_literals_alone() {
        let source = credential_fixture(
            "from flask import request\nFIELD = request.form['FIELD']\nFIELD = 'independent-literal'\n",
        );
        let proofs = ProvenLookups::new("auth.py", source.as_bytes());
        let assignment = format!("{} = ", "password");
        let first = source.find(&assignment).unwrap();
        let last = source.rfind(&assignment).unwrap();
        assert!(proofs.contains(first, first + assignment.len()));
        assert!(!proofs.contains(last, last + assignment.len()));
        assert!(!proofs.contains(first + 8, first + assignment.len()));
    }

    #[test]
    fn a_credential_suffix_does_not_prove_the_complete_assignment_key() {
        let key = ["unknown_prefix_", "password"].concat();
        let source = format!("from flask import request\n{key} = request.form['{key}']\n");
        let proofs = ProvenLookups::new("auth.py", source.as_bytes());
        let key_start = source.find(&format!("{key} = ")).unwrap();
        let value_start = key_start + key.len() + " = ".len();
        let match_start = key_start + "unknown_prefix_".len();
        assert!(!proofs.contains(match_start, value_start));
    }

    #[test]
    fn both_scanners_keep_suffixes_and_commented_lookup_payloads_withheld() {
        use super::super::{
            CONTEXT_ASSIGNMENT_RULE_ID, SecretScan, SecretSpansScan, scan_secret_bytes,
            scan_secret_spans,
        };

        let canonical =
            credential_fixture("from flask import request\nFIELD = request.form['FIELD']\n");
        assert_eq!(
            scan_secret_bytes("auth.py", canonical.as_bytes()),
            SecretScan::Clean
        );
        assert!(matches!(
            scan_secret_spans("auth.py", canonical.as_bytes()),
            SecretSpansScan::Spans(spans) if spans.is_empty()
        ));

        let suffix = ["unknown_prefix_", "password"].concat();
        let canary = ["Lookup", "Canary", "9284"].concat();
        let mut controls = vec![format!(
            "from flask import request\n{suffix} = request.form['{suffix}']\n"
        )];
        // A leading comment has no legacy regex match; its proof denial is covered above.
        controls.extend(
            [
                "from flask import request\nFIELD = (request.form['FIELD'] # 'CANARY'\n)\n",
                "from flask import request\nFIELD = (request.form # 'CANARY'\n    ['FIELD'])\n",
                "from flask import request\nFIELD = (request # 'CANARY'\n    .form['FIELD'])\n",
                "from flask import request\nFIELD = request.form['FIELD' # 'CANARY'\n]\n",
            ]
            .map(|template| credential_fixture(template).replace("CANARY", &canary)),
        );
        for (index, source) in controls.iter().enumerate() {
            assert!(
                matches!(
                    scan_secret_bytes("auth.py", source.as_bytes()),
                    SecretScan::Sensitive { rule_ids, .. } if rule_ids.contains(&CONTEXT_ASSIGNMENT_RULE_ID)
                ),
                "byte scanner control {index}"
            );
            assert!(
                matches!(
                    scan_secret_spans("auth.py", source.as_bytes()),
                    SecretSpansScan::Spans(spans) if spans.iter().any(|span| span.rule_id == CONTEXT_ASSIGNMENT_RULE_ID)
                ),
                "span scanner control {index}"
            );
        }
    }
}
