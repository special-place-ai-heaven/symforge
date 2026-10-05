//! Bounded, transient source evidence for reference queries.
//!
//! This module does not persist AST-derived data. It reparses indexed source
//! bytes and returns only declaration headers whose exact AST ranges validate.

use crate::domain::index::LanguageId;
use tree_sitter::{Node, Tree};

const MAX_EVIDENCE_SOURCE_BYTES: usize = 2 * 1024 * 1024;
const MAX_HEADER_BYTES: usize = 1024;
const MAX_HEADER_LINES: usize = 10;
const MAX_IDENTITY_BYTES: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeclarationIdentity {
    pub(crate) name: String,
    pub(crate) byte_range: (u32, u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeclarationHeader {
    pub(crate) node_kind: String,
    pub(crate) is_callable: bool,
    /// Exact full declaration span, including the body and any template wrappers.
    pub(crate) definition_byte_range: (u32, u32),
    pub(crate) definition_line_range: (u32, u32),
    /// Exact source slice containing the declaration prefix, through body start.
    pub(crate) byte_range: (u32, u32),
    pub(crate) line_number: u32,
    /// First source line of the callable syntax after leading attributes,
    /// annotations, decorators, or template wrappers. `None` means the
    /// grammar did not expose an unambiguous signature start.
    pub(crate) signature_start_line: Option<u32>,
    /// A grammar-declared identifier node inside the verified header, if any.
    pub(crate) identity: Option<DeclarationIdentity>,
    pub(crate) text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct ReferenceEvidence {
    pub(crate) caller: Option<DeclarationHeader>,
    pub(crate) lexical_parents: Vec<DeclarationHeader>,
}

pub(crate) struct ReferenceEvidenceIndex<'a> {
    source: &'a str,
    tree: Tree,
}

impl<'a> ReferenceEvidenceIndex<'a> {
    pub(crate) fn new(source: &'a [u8], language: &LanguageId, is_tsx: bool) -> Option<Self> {
        if source.len() > MAX_EVIDENCE_SOURCE_BYTES {
            return None;
        }
        let source = std::str::from_utf8(source).ok()?;
        let tree = crate::parsing::parse_source_tree(source, language, is_tsx).ok()?;
        Some(Self { source, tree })
    }

    pub(crate) fn for_reference(&self, byte_range: (u32, u32)) -> ReferenceEvidence {
        let start = byte_range.0 as usize;
        let end = byte_range.1 as usize;
        if start >= end
            || end > self.source.len()
            || !self.source.is_char_boundary(start)
            || !self.source.is_char_boundary(end)
        {
            return ReferenceEvidence::default();
        }

        let mut ancestry = Vec::new();
        if !find_containing_path(self.tree.root_node(), start, end, &mut ancestry) {
            return ReferenceEvidence::default();
        }
        let Some(caller_index) = ancestry
            .iter()
            .rposition(|node| is_callable_kind(node.kind()))
        else {
            return ReferenceEvidence::default();
        };
        let caller_node = ancestry[caller_index];
        // A recovered callable can have a clean-looking prefix while its body
        // span (and thus caller association) was extended by error recovery.
        if caller_node.has_error() {
            return ReferenceEvidence::default();
        }
        let Some(caller) = declaration_header(self.source, caller_node) else {
            return ReferenceEvidence::default();
        };

        // Only actual AST ancestors may supply parent identities. An errorful
        // parent terminates enrichment; a clean inner caller can still survive.
        let mut lexical_parents = Vec::new();
        for node in ancestry[..caller_index].iter().rev().copied() {
            if !is_type_kind(node.kind()) && !is_callable_kind(node.kind()) {
                continue;
            }
            if node.has_error() {
                break;
            }
            let Some(header) = declaration_header(self.source, node) else {
                break;
            };
            lexical_parents.push(header);
        }
        ReferenceEvidence {
            caller: Some(caller),
            lexical_parents,
        }
    }

    /// Return a header only when the nearest AST declaration span exactly
    /// matches the indexed symbol span. This prevents a variable or field from
    /// inheriting its containing method's declaration as its own definition.
    pub(crate) fn for_declaration(&self, byte_range: (u32, u32)) -> Option<DeclarationHeader> {
        let start = byte_range.0 as usize;
        let end = byte_range.1 as usize;
        if start >= end || end > self.source.len() {
            return None;
        }
        let mut ancestry = Vec::new();
        if !find_containing_path(self.tree.root_node(), start, end, &mut ancestry) {
            return None;
        }
        let node = ancestry
            .iter()
            .rev()
            .copied()
            .find(|node| is_callable_kind(node.kind()) || is_type_kind(node.kind()))?;
        (node.byte_range() == (start..end))
            .then(|| declaration_header(self.source, node))
            .flatten()
    }
}

fn find_containing_path<'tree>(
    node: Node<'tree>,
    start: usize,
    end: usize,
    path: &mut Vec<Node<'tree>>,
) -> bool {
    let range = node.byte_range();
    if range.start > start || range.end < end {
        return false;
    }
    path.push(node);
    for index in 0..node.child_count() {
        let Some(child) = node.child(index as u32) else {
            continue;
        };
        let child_range = child.byte_range();
        if child_range.start <= start
            && child_range.end >= end
            && find_containing_path(child, start, end, path)
        {
            return true;
        }
    }
    true
}

fn declaration_header(source: &str, node: Node<'_>) -> Option<DeclarationHeader> {
    if node.is_error() || node.is_missing() {
        return None;
    }
    let mut wrapper = node;
    while let Some(parent) = wrapper
        .parent()
        .filter(|parent| parent.kind() == "template_declaration")
    {
        wrapper = parent;
    }
    let start = wrapper.start_byte();
    let end = node.end_byte();
    let body = node.child_by_field_name("body");
    if body.is_none() && is_callable_definition_kind(node.kind()) {
        return None;
    }
    let body_start = body.map(|body| body.start_byte()).unwrap_or(end);
    if start >= body_start || body_start > end || body_start - start > MAX_HEADER_BYTES {
        return None;
    }

    let header = source.get(start..body_start)?;
    if header.lines().count() > MAX_HEADER_LINES
        || !header_region_is_clean(wrapper, start, body_start)
    {
        return None;
    }
    if header.trim().is_empty() {
        return None;
    }
    let signature_start_line = is_callable_kind(node.kind())
        .then(|| callable_signature_start_byte(node))
        .flatten()
        .map(|byte| line_for_byte(source, byte));
    let identity = declaration_identity(source, node, start, body_start);
    Some(DeclarationHeader {
        node_kind: node.kind().to_string(),
        is_callable: is_callable_kind(node.kind()),
        definition_byte_range: (start as u32, end as u32),
        definition_line_range: (
            (wrapper.start_position().row + 1) as u32,
            line_for_byte(source, end.saturating_sub(1)),
        ),
        byte_range: (start as u32, body_start as u32),
        line_number: (wrapper.start_position().row + 1) as u32,
        signature_start_line,
        identity,
        text: header.to_string(),
    })
}

fn declaration_identity(
    source: &str,
    node: Node<'_>,
    header_start: usize,
    header_end: usize,
) -> Option<DeclarationIdentity> {
    let name = node.child_by_field_name("name")?;
    if name.has_error()
        || name.is_missing()
        || name.child_count() != 0
        || !matches!(
            name.kind(),
            "identifier" | "type_identifier" | "property_identifier" | "field_identifier"
        )
    {
        return None;
    }
    let range = name.byte_range();
    if range.start < header_start
        || range.end > header_end
        || range.start >= range.end
        || range.end - range.start > MAX_IDENTITY_BYTES
    {
        return None;
    }
    let text = source.get(range.clone())?;
    if text.contains(['\n', '\r']) {
        return None;
    }
    Some(DeclarationIdentity {
        name: text.to_string(),
        byte_range: (
            u32::try_from(range.start).ok()?,
            u32::try_from(range.end).ok()?,
        ),
    })
}

fn callable_signature_start_byte(node: Node<'_>) -> Option<usize> {
    let body_start = node
        .child_by_field_name("body")
        .map_or(node.end_byte(), |body| body.start_byte());
    for index in 0..node.child_count() {
        let child = node.child(index as u32)?;
        if child.start_byte() >= body_start {
            break;
        }
        if child.is_error() || child.is_missing() || child.has_error() {
            return None;
        }
        if is_leading_declaration_metadata(child.kind()) {
            continue;
        }
        if child.kind() == "modifiers" {
            match first_non_metadata_child_start(child, body_start) {
                Ok(Some(start)) => return Some(start),
                Ok(None) => continue,
                Err(()) => return None,
            }
        }
        return Some(child.start_byte());
    }
    None
}

fn first_non_metadata_child_start(node: Node<'_>, before: usize) -> Result<Option<usize>, ()> {
    for index in 0..node.child_count() {
        let Some(child) = node.child(index as u32) else {
            return Err(());
        };
        if child.start_byte() >= before {
            break;
        }
        if child.is_error() || child.is_missing() || child.has_error() {
            return Err(());
        }
        if is_leading_declaration_metadata(child.kind()) {
            continue;
        }
        if child.kind() == "modifiers" {
            match first_non_metadata_child_start(child, before)? {
                Some(start) => return Ok(Some(start)),
                None => continue,
            }
        }
        return Ok(Some(child.start_byte()));
    }
    Ok(None)
}

fn is_leading_declaration_metadata(kind: &str) -> bool {
    matches!(
        kind,
        "annotation"
            | "marker_annotation"
            | "attribute"
            | "attribute_list"
            | "attribute_item"
            | "attribute_specifier"
            | "decorator"
            | "block_comment"
            | "comment"
            | "line_comment"
            | "template_declaration"
    )
}

fn line_for_byte(source: &str, byte: usize) -> u32 {
    source
        .as_bytes()
        .get(..byte.min(source.len()))
        .unwrap_or_default()
        .iter()
        .filter(|&&byte| byte == b'\n')
        .count()
        .saturating_add(1) as u32
}

fn header_region_is_clean(node: Node<'_>, start: usize, end: usize) -> bool {
    let range = node.byte_range();
    if range.end < start || range.start >= end {
        return true;
    }
    if node.is_error() || node.is_missing() {
        return false;
    }
    (0..node.child_count()).all(|index| {
        node.child(index as u32)
            .is_none_or(|child| header_region_is_clean(child, start, end))
    })
}

fn is_callable_kind(kind: &str) -> bool {
    matches!(
        kind,
        "function_definition"
            | "function_declaration"
            | "function_item"
            | "function_signature_item"
            | "method_definition"
            | "method_declaration"
            | "constructor_declaration"
            | "constructor_definition"
            | "local_function_statement"
            | "function"
            | "method"
            | "lambda_expression"
    )
}

fn is_callable_definition_kind(kind: &str) -> bool {
    matches!(
        kind,
        "function_definition"
            | "function_item"
            | "method_definition"
            | "constructor_definition"
            | "local_function_statement"
            | "function"
            | "method"
            | "lambda_expression"
    )
}

fn is_type_kind(kind: &str) -> bool {
    matches!(
        kind,
        "class_specifier"
            | "struct_specifier"
            | "enum_specifier"
            | "class_declaration"
            | "interface_declaration"
            | "class_definition"
            | "struct_declaration"
            | "type_declaration"
            | "impl_item"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_and_text_languages_do_not_claim_ast_evidence() {
        for (language, source) in [
            (LanguageId::Json, "{\"name\":\"sample\"}"),
            (LanguageId::Toml, "name = \"sample\"\n"),
            (LanguageId::Yaml, "name: sample\n"),
            (LanguageId::Markdown, "# Sample\n"),
            (LanguageId::Text, "sample\n"),
            (LanguageId::Env, "NAME=sample\n"),
        ] {
            assert!(
                ReferenceEvidenceIndex::new(source.as_bytes(), &language, false).is_none(),
                "{} must not provide code AST evidence",
                language.name()
            );
            assert!(crate::parsing::parse_source_tree(source, &language, false).is_err());
        }
    }

    fn evidence(source: &str, query: &str, language: LanguageId) -> ReferenceEvidence {
        let index = ReferenceEvidenceIndex::new(source.as_bytes(), &language, false).unwrap();
        let at = source.find(query).unwrap();
        index.for_reference((at as u32, (at + query.len()) as u32))
    }

    #[test]
    fn cpp_template_caller_header_includes_template_wrapper_and_excludes_body() {
        let source = "template <class T>\nstruct box {\n  template <class U>\n  auto get(U value) const { return value; }\n};\nvoid use(box<int>& b) { b.get(1); }\n";
        let evidence = evidence(source, "get(1)", LanguageId::Cpp);
        let caller = evidence.caller.unwrap();
        assert!(caller.text.contains("void use(box<int>& b)"));
        assert!(!caller.text.contains("b.get(1)"));
        assert!(caller.definition_byte_range.1 > caller.byte_range.1);
        assert_eq!(
            source.get(
                caller.definition_byte_range.0 as usize..caller.definition_byte_range.1 as usize
            ),
            Some("void use(box<int>& b) { b.get(1); }")
        );
        assert_eq!(caller.definition_line_range, (6, 6));
    }

    #[test]
    fn cpp_recovered_ancestor_is_omitted_but_clean_inner_header_survives() {
        let source = "struct broken {\n  int field = ;\n  void call() { target(); }\n};\n";
        let evidence = evidence(source, "target()", LanguageId::Cpp);
        let caller = evidence
            .caller
            .expect("clean inner declaration should be available");
        assert!(caller.text.contains("void call()"));
        assert!(evidence.lexical_parents.is_empty());
    }

    #[test]
    fn errorful_callable_does_not_claim_a_clean_callsite_as_its_caller() {
        let source = "void caller() { int broken = ; call(); }\n";
        let evidence = evidence(source, "call()", LanguageId::Cpp);
        assert!(evidence.caller.is_none());
        assert!(evidence.lexical_parents.is_empty());
    }

    #[test]
    fn csharp_local_function_is_selected_as_the_exact_caller() {
        let source = "class Mapper {\n  async Task Outer<T>() {\n    async IAsyncEnumerable<T> Impl(int value) {\n      Target(value);\n      await Task.Yield();\n      yield return default!;\n    }\n  }\n}\n";
        let mut parser = tree_sitter::Parser::new();
        let language: tree_sitter::Language = tree_sitter_c_sharp::LANGUAGE.into();
        parser.set_language(&language).expect("set C# grammar");
        let tree = parser.parse(source, None).expect("parse C# fixture");
        let root = tree.root_node();
        assert!(!root.has_error(), "fixture should produce a clean C# AST");

        fn find_kind<'tree>(node: Node<'tree>, kind: &str) -> Option<Node<'tree>> {
            if node.kind() == kind {
                return Some(node);
            }
            (0..node.child_count()).find_map(|index| find_kind(node.child(index as u32)?, kind))
        }

        let local = find_kind(root, "local_function_statement")
            .expect("C# grammar should represent Impl as local_function_statement");
        assert!(!local.has_error());
        let body = local
            .child_by_field_name("body")
            .expect("local function grammar exposes a body field");
        let expected_header = &source[local.start_byte()..body.start_byte()];
        assert!(expected_header.contains("async IAsyncEnumerable<T> Impl(int value)"));
        assert!(!expected_header.contains("Impl<T>"));

        let evidence = evidence(source, "Target(value)", LanguageId::CSharp);
        let caller = evidence
            .caller
            .expect("local caller evidence should be available");
        assert_eq!(caller.node_kind, "local_function_statement");
        assert_eq!(caller.text, expected_header);
        assert_eq!(
            source.get(caller.byte_range.0 as usize..caller.byte_range.1 as usize),
            Some(caller.text.as_str())
        );
        assert_eq!(
            source.get(
                caller.definition_byte_range.0 as usize..caller.definition_byte_range.1 as usize
            ),
            Some(&source[local.start_byte()..local.end_byte()])
        );
        assert_eq!(
            caller.definition_line_range,
            (
                local.start_position().row as u32 + 1,
                local.end_position().row as u32 + 1
            )
        );
    }

    #[test]
    fn java_signature_anchor_skips_multiple_multiline_annotations() {
        let source = "class Calls {\n  @First(\n    value = \"x\"\n  )\n  @Second\n  public void annotated() {\n    firstTarget();\n  }\n  @Inline public void sameLine() { secondTarget(); }\n  public\n  void multiline() { thirdTarget(); }\n}\n";

        let annotated = evidence(source, "firstTarget()", LanguageId::Java)
            .caller
            .expect("annotated caller should be available");
        let annotated_identity = annotated.identity.as_ref().expect("Java name field");
        assert_eq!(annotated_identity.name, "annotated");
        assert_eq!(
            source.get(
                annotated_identity.byte_range.0 as usize..annotated_identity.byte_range.1 as usize
            ),
            Some("annotated")
        );
        assert_eq!(annotated.line_number, 2);
        assert_eq!(annotated.signature_start_line, Some(6));
        assert_eq!(annotated.definition_line_range.0, 2);
        assert!(
            annotated.text.contains("@First("),
            "header: {:?}",
            annotated.text
        );
        assert!(annotated.text.contains("  @Second\n"));
        assert_eq!(
            source.get(annotated.byte_range.0 as usize..annotated.byte_range.1 as usize),
            Some(annotated.text.as_str())
        );
        assert_eq!(
            source.get(
                annotated.definition_byte_range.0 as usize
                    ..annotated.definition_byte_range.1 as usize
            ),
            Some(&source[source.find("@First").unwrap()..source.find("\n  @Inline").unwrap()])
        );

        let same_line = evidence(source, "secondTarget()", LanguageId::Java)
            .caller
            .expect("same-line annotated caller should be available");
        assert_eq!(same_line.line_number, 9);
        assert_eq!(same_line.signature_start_line, Some(9));
        assert_eq!(
            source.get(same_line.byte_range.0 as usize..same_line.byte_range.1 as usize),
            Some(same_line.text.as_str())
        );

        let multiline = evidence(source, "thirdTarget()", LanguageId::Java)
            .caller
            .expect("multiline signature caller should be available");
        assert_eq!(multiline.signature_start_line, Some(10));
        assert_eq!(
            multiline
                .identity
                .as_ref()
                .map(|identity| identity.name.as_str()),
            Some("multiline")
        );
    }

    #[test]
    fn java_signature_anchor_handles_annotation_only_modifiers_and_comments() {
        let annotation_only = "class Calls {\n  @Anno\n  void f() { target(); }\n}\n";
        let mut parser = tree_sitter::Parser::new();
        let language: tree_sitter::Language = tree_sitter_java::LANGUAGE.into();
        parser.set_language(&language).expect("set Java grammar");
        let tree = parser
            .parse(annotation_only, None)
            .expect("parse annotation-only Java fixture");
        let caller = evidence(annotation_only, "target()", LanguageId::Java)
            .caller
            .expect("annotation-only caller should be available");
        assert_eq!(caller.line_number, 2);
        assert_eq!(
            caller.signature_start_line,
            Some(3),
            "AST child shape: {}",
            tree.root_node().to_sexp()
        );

        let with_comment = "class Calls {\r\n  @Anno(\"λ\")\r\n  // note Ω\r\n  public\r\n  void f() { target(); }\r\n}\r\n";
        let caller = evidence(with_comment, "target()", LanguageId::Java)
            .caller
            .expect("comment-separated annotated caller should be available");
        assert_eq!(caller.line_number, 2);
        assert_eq!(caller.signature_start_line, Some(4));
        assert_eq!(
            with_comment.get(caller.byte_range.0 as usize..caller.byte_range.1 as usize),
            Some(caller.text.as_str())
        );
    }

    #[test]
    fn callable_signature_scan_rejects_errorful_prefix_but_keeps_clean_prefix_with_body_error() {
        fn first_kind<'tree>(node: Node<'tree>, kind: &str) -> Option<Node<'tree>> {
            if node.kind() == kind {
                return Some(node);
            }
            (0..node.child_count()).find_map(|index| first_kind(node.child(index as u32)?, kind))
        }

        let malformed_prefix = "class C { @Anno(1 + ) void f() { target(); } }";
        let mut parser = tree_sitter::Parser::new();
        let java: tree_sitter::Language = tree_sitter_java::LANGUAGE.into();
        parser.set_language(&java).expect("set Java grammar");
        let tree = parser
            .parse(malformed_prefix, None)
            .expect("parse malformed-prefix fixture");
        let method = first_kind(tree.root_node(), "method_declaration").unwrap_or_else(|| {
            panic!(
                "recovered Java method node should be present: {}",
                tree.root_node().to_sexp()
            )
        });
        assert!(
            method.has_error(),
            "malformed annotation must remain an errorful prefix: {}",
            tree.root_node().to_sexp()
        );
        assert!(callable_signature_start_byte(method).is_none());

        let body_error = "void f() { int broken = ; target(); }";
        let cpp: tree_sitter::Language = tree_sitter_cpp::LANGUAGE.into();
        parser.set_language(&cpp).expect("set C++ grammar");
        let tree = parser
            .parse(body_error, None)
            .expect("parse body-error fixture");
        let function = first_kind(tree.root_node(), "function_definition")
            .expect("C++ function node should be present");
        assert!(
            function.has_error(),
            "fixture must contain a body parse error"
        );
        assert_eq!(
            callable_signature_start_byte(function),
            Some(body_error.find("void f").unwrap())
        );
    }

    #[test]
    fn supported_language_signature_anchors_skip_leading_syntax_wrappers() {
        let csharp = "class C {\n  async Task Outer() {\n    [System.Obsolete]\n    static async Task Impl() { Target(); }\n  }\n}\n";
        let caller = evidence(csharp, "Target()", LanguageId::CSharp)
            .caller
            .expect("attribute-decorated local function should be available");
        assert_eq!(caller.node_kind, "local_function_statement");
        assert_eq!(caller.line_number, 3);
        assert_eq!(caller.signature_start_line, Some(4));
        assert!(caller.text.contains("[System.Obsolete]"));
        assert_eq!(
            csharp.get(caller.byte_range.0 as usize..caller.byte_range.1 as usize),
            Some(caller.text.as_str())
        );

        let python =
            "def outer():\n    @first\n    @second()\n    def inner():\n        target()\n";
        let caller = evidence(python, "target()", LanguageId::Python)
            .caller
            .expect("decorated Python callable should be available");
        assert_eq!(caller.signature_start_line, Some(4));

        let rust = "#[inline]\nfn caller() { target(); }\n";
        let caller = evidence(rust, "target()", LanguageId::Rust)
            .caller
            .expect("attributed Rust callable should be available");
        assert_eq!(caller.signature_start_line, Some(2));
        let identity = caller.identity.as_ref().expect("Rust name field");
        assert_eq!(identity.name, "caller");
        assert_eq!(
            rust.get(identity.byte_range.0 as usize..identity.byte_range.1 as usize),
            Some("caller")
        );

        let cpp = "template <class T>\nauto caller(T value) { target(); }\n";
        let caller = evidence(cpp, "target()", LanguageId::Cpp)
            .caller
            .expect("templated C++ callable should be available");
        assert_eq!(caller.line_number, 1);
        assert_eq!(caller.signature_start_line, Some(2));
        assert!(caller.text.starts_with("template <class T>"));
        assert_eq!(
            cpp.get(caller.byte_range.0 as usize..caller.byte_range.1 as usize),
            Some(caller.text.as_str())
        );
    }

    #[test]
    fn unicode_caller_identity_uses_exact_utf8_byte_span() {
        let source = "fn café() { target(); }\n";
        let caller = evidence(source, "target()", LanguageId::Rust)
            .caller
            .expect("Unicode-named Rust caller should be available");
        let identity = caller.identity.expect("grammar-declared caller name");
        assert_eq!(identity.name, "café");
        assert_eq!(identity.byte_range.0 as usize, source.find("café").unwrap());
        assert_eq!(
            identity.byte_range.1 - identity.byte_range.0,
            "café".len() as u32
        );
        assert_eq!(
            source.get(identity.byte_range.0 as usize..identity.byte_range.1 as usize),
            Some("café")
        );
    }

    #[test]
    fn signature_anchor_is_unavailable_for_non_callable_declarations() {
        let source = "struct Target {};\n";
        let index =
            ReferenceEvidenceIndex::new(source.as_bytes(), &LanguageId::Cpp, false).unwrap();
        let mut parser = tree_sitter::Parser::new();
        let language: tree_sitter::Language = tree_sitter_cpp::LANGUAGE.into();
        parser.set_language(&language).expect("set C++ grammar");
        let tree = parser.parse(source, None).expect("parse C++ fixture");
        fn find_kind<'tree>(node: Node<'tree>, kind: &str) -> Option<Node<'tree>> {
            if node.kind() == kind {
                return Some(node);
            }
            (0..node.child_count()).find_map(|index| find_kind(node.child(index as u32)?, kind))
        }
        let declaration = find_kind(tree.root_node(), "struct_specifier")
            .expect("C++ grammar should expose the struct declaration");
        let header = index
            .for_declaration((
                declaration.start_byte() as u32,
                declaration.end_byte() as u32,
            ))
            .expect("exact indexed type declaration header should be available");
        assert!(!header.is_callable);
        assert_eq!(header.signature_start_line, None);
    }

    #[test]
    fn rust_and_typescript_callable_headers_stop_at_the_body_field() {
        for (language, source, header, definition) in [
            (
                LanguageId::Rust,
                "fn caller() { target(); }",
                "fn caller() ",
                "fn caller() { target(); }",
            ),
            (
                LanguageId::TypeScript,
                "function caller() { target(); }",
                "function caller() ",
                "function caller() { target(); }",
            ),
        ] {
            let evidence = evidence(source, "target()", language);
            let caller = evidence.caller.expect("callable metadata available");
            assert_eq!(caller.text, header);
            assert_eq!(
                source.get(
                    caller.definition_byte_range.0 as usize
                        ..caller.definition_byte_range.1 as usize
                ),
                Some(definition)
            );
        }
    }

    #[test]
    fn invalid_ranges_and_non_utf8_are_omitted() {
        let source = "void call() { target(); }";
        let index =
            ReferenceEvidenceIndex::new(source.as_bytes(), &LanguageId::Cpp, false).unwrap();
        assert_eq!(index.for_reference((3, 3)), ReferenceEvidence::default());
        assert!(ReferenceEvidenceIndex::new(&[0xff], &LanguageId::Cpp, false).is_none());
    }

    #[test]
    fn cpp_specialization_header_preserves_template_and_crlf_bytes() {
        let source = "template <class T>\r\nstruct formatter<complex<T>> {\r\n  auto format(const complex<T>& value = complex<T>{}) const { call(); }\r\n};\r\n";
        let index =
            ReferenceEvidenceIndex::new(source.as_bytes(), &LanguageId::Cpp, false).unwrap();
        let start = source.find("struct formatter").unwrap();
        let end = source.find("\r\n};").unwrap() + 3;
        let header = index.for_declaration((start as u32, end as u32)).unwrap();
        assert!(
            header
                .text
                .contains("template <class T>\r\nstruct formatter<complex<T>>")
        );
        assert!(!header.text.contains("auto format"));
        assert_eq!(
            source.get(header.byte_range.0 as usize..header.byte_range.1 as usize),
            Some(header.text.as_str())
        );
        let caller = evidence(source, "call()", LanguageId::Cpp).caller.unwrap();
        assert!(caller.text.contains("complex<T>{}"));
        assert!(!caller.text.contains("call();"));
    }

    #[test]
    fn out_of_class_member_header_preserves_nested_template_wrappers() {
        let source = "template <class T>\ntemplate <class U>\nvoid owner<T>::method(U value) { invoke(); }\n";
        let caller = evidence(source, "invoke()", LanguageId::Cpp)
            .caller
            .expect("valid out-of-class member header");
        assert!(
            caller
                .text
                .contains("template <class T>\ntemplate <class U>")
        );
        assert!(caller.text.contains("void owner<T>::method(U value)"));
        assert!(!caller.text.contains("invoke();"));
        assert_eq!(
            source.get(caller.byte_range.0 as usize..caller.byte_range.1 as usize),
            Some(caller.text.as_str())
        );
        assert_eq!(
            source.get(
                caller.definition_byte_range.0 as usize..caller.definition_byte_range.1 as usize
            ),
            Some(
                "template <class T>\ntemplate <class U>\nvoid owner<T>::method(U value) { invoke(); }"
            )
        );
        assert_eq!(caller.definition_line_range, (1, 3));
    }

    #[test]
    fn nested_variable_candidate_cannot_inherit_its_enclosing_function_header() {
        let source = "void caller() { int target = 1; use(target); }";
        let index =
            ReferenceEvidenceIndex::new(source.as_bytes(), &LanguageId::Cpp, false).unwrap();
        let start = source.find("target").unwrap();
        assert!(
            index
                .for_declaration((start as u32, (start + "target".len()) as u32))
                .is_none()
        );
    }

    #[test]
    fn malformed_or_oversize_source_does_not_claim_declaration_evidence() {
        let malformed = "void caller( { call(); }";
        let index =
            ReferenceEvidenceIndex::new(malformed.as_bytes(), &LanguageId::Cpp, false).unwrap();
        let start = malformed.find("call").unwrap();
        let evidence = index.for_reference((start as u32, (start + 4) as u32));
        assert!(evidence.caller.is_none());
        assert!(evidence.lexical_parents.is_empty());

        let mut oversized = vec![b' '; MAX_EVIDENCE_SOURCE_BYTES + 1];
        oversized[..14].copy_from_slice(b"void call() {}");
        assert!(ReferenceEvidenceIndex::new(&oversized, &LanguageId::Cpp, false).is_none());
    }
}
