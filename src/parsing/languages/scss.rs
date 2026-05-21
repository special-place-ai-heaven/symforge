use tree_sitter::Node;

use super::{NO_DOC_SPEC, SymbolSink, at_rule_name, collect_symbols, push_symbol};
use crate::domain::{SymbolKind, SymbolRecord};

pub fn extract_symbols(node: &Node, source: &str) -> Vec<SymbolRecord> {
    collect_symbols(node, source, walk_node)
}

fn walk_node(
    node: &Node,
    source: &str,
    depth: u32,
    sort_order: &mut u32,
    symbols: &mut Vec<SymbolRecord>,
) {
    match node.kind() {
        "rule_set" => {
            // Extract full selector text by child kind (not field name).
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "selectors" {
                    let name = child
                        .utf8_text(source.as_bytes())
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    if !name.is_empty() {
                        let mut sink = SymbolSink::new(source, sort_order, symbols, &NO_DOC_SPEC);
                        push_symbol(node, name, SymbolKind::Other, depth, &mut sink);
                    }
                    break;
                }
            }
            // Recurse into block for custom properties and nested rules.
            walk_children(node, source, depth + 1, sort_order, symbols);
        }
        "declaration" => {
            // Both SCSS $variables and CSS custom properties use "declaration"
            // with a "property_name" child. Distinguish by prefix.
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "property_name" || child.kind() == "variable" {
                    let text = child.utf8_text(source.as_bytes()).unwrap_or("");
                    if text.starts_with('$') || text.starts_with("--") {
                        let mut sink = SymbolSink::new(source, sort_order, symbols, &NO_DOC_SPEC);
                        push_symbol(
                            node,
                            text.to_string(),
                            SymbolKind::Variable,
                            depth,
                            &mut sink,
                        );
                    }
                    break;
                }
            }
        }
        "mixin_statement" => {
            if let Some(name) = find_identifier_child(node, source) {
                let mut sink = SymbolSink::new(source, sort_order, symbols, &NO_DOC_SPEC);
                push_symbol(node, name, SymbolKind::Function, depth, &mut sink);
            }
            walk_children(node, source, depth + 1, sort_order, symbols);
        }
        "function_statement" => {
            if let Some(name) = find_identifier_child(node, source) {
                let mut sink = SymbolSink::new(source, sort_order, symbols, &NO_DOC_SPEC);
                push_symbol(node, name, SymbolKind::Function, depth, &mut sink);
            }
            walk_children(node, source, depth + 1, sort_order, symbols);
        }
        // Skip @include, @use, @forward, @extend — call sites / imports, not definitions.
        // @extend is an extend_statement node; silenced because it references a
        // definition elsewhere, similar to @include.
        "include_statement" | "use_statement" | "forward_statement" | "extend_statement"
        | "at_rule" => {}
        "media_statement" => {
            let name = at_rule_name(node, source);
            if !name.is_empty() {
                let mut sink = SymbolSink::new(source, sort_order, symbols, &NO_DOC_SPEC);
                push_symbol(node, name, SymbolKind::Module, depth, &mut sink);
            }
            walk_children(node, source, depth + 1, sort_order, symbols);
        }
        "keyframes_statement" => {
            let name = at_rule_name(node, source);
            if !name.is_empty() {
                let mut sink = SymbolSink::new(source, sort_order, symbols, &NO_DOC_SPEC);
                push_symbol(node, name, SymbolKind::Module, depth, &mut sink);
            }
            // Do NOT recurse — skip inner keyframe steps.
        }
        _ => {
            walk_children(node, source, depth, sort_order, symbols);
        }
    }
}

/// Find the first `identifier` child of a node and return its text.
fn find_identifier_child(node: &Node, source: &str) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" {
            let text = child.utf8_text(source.as_bytes()).unwrap_or("").to_string();
            if !text.is_empty() {
                return Some(text);
            }
        }
    }
    None
}

/// Walk all children of a node.
fn walk_children(
    node: &Node,
    source: &str,
    depth: u32,
    sort_order: &mut u32,
    symbols: &mut Vec<SymbolRecord>,
) {
    let Some(_frame) = super::enter_ast_walk_frame() else {
        return;
    };
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_node(&child, source, depth, sort_order, symbols);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{LanguageId, SymbolKind};
    use crate::parsing::inline_tests::inline_test;
    use tree_sitter::Parser;

    inline_test!(
        scss_inline_test_extracts_variable,
        LanguageId::Scss,
        r#"
$inline-scss-probe: #333;
"#,
        [(SymbolKind::Variable, "$inline-scss-probe")]
    );

    fn parse_scss(source: &str) -> Vec<SymbolRecord> {
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_scss::language();
        parser.set_language(&lang).expect("set SCSS language");
        let tree = parser.parse(source, None).expect("parse SCSS source");
        extract_symbols(&tree.root_node(), source)
    }

    #[test]
    fn test_scss_variable_extracted() {
        let symbols = parse_scss("$primary-color: #333;");
        let var = symbols.iter().find(|s| s.kind == SymbolKind::Variable);
        assert!(
            var.is_some(),
            "should extract $variable as Variable, got: {:?}",
            symbols
        );
        assert_eq!(var.unwrap().name, "$primary-color");
    }

    #[test]
    fn test_scss_mixin_extracted() {
        let symbols = parse_scss("@mixin button-base { display: inline; }");
        let mixin = symbols.iter().find(|s| s.kind == SymbolKind::Function);
        assert!(
            mixin.is_some(),
            "should extract @mixin as Function, got: {:?}",
            symbols
        );
        assert_eq!(mixin.unwrap().name, "button-base");
    }

    #[test]
    fn test_scss_function_extracted() {
        let symbols = parse_scss("@function darken-color($color) { @return $color; }");
        let func = symbols.iter().find(|s| s.kind == SymbolKind::Function);
        assert!(
            func.is_some(),
            "should extract @function as Function, got: {:?}",
            symbols
        );
        assert_eq!(func.unwrap().name, "darken-color");
    }

    #[test]
    fn test_scss_include_not_extracted() {
        let symbols = parse_scss("@include button-base;");
        assert!(
            symbols.is_empty(),
            "@include should not be extracted, got: {:?}",
            symbols
        );
    }

    #[test]
    fn test_scss_use_forward_not_extracted() {
        let symbols = parse_scss("@use 'variables';\n@forward 'mixins';");
        assert!(
            symbols.is_empty(),
            "@use/@forward should not be extracted, got: {:?}",
            symbols
        );
    }

    #[test]
    fn test_scss_css_selectors_also_work() {
        let symbols = parse_scss(".btn { color: red; }");
        let rule = symbols.iter().find(|s| s.kind == SymbolKind::Other);
        assert!(
            rule.is_some(),
            "should extract CSS selector as Other, got: {:?}",
            symbols
        );
        assert_eq!(rule.unwrap().name, ".btn");
    }

    #[test]
    fn test_scss_custom_property_extracted() {
        let symbols = parse_scss(":root { --gap: 8px; }");
        let var = symbols.iter().find(|s| s.kind == SymbolKind::Variable);
        assert!(
            var.is_some(),
            "should extract custom property as Variable, got: {:?}",
            symbols
        );
        assert_eq!(var.unwrap().name, "--gap");
    }

    #[test]
    fn test_scss_empty_file() {
        let symbols = parse_scss("");
        assert!(symbols.is_empty(), "empty file should produce zero symbols");
    }

    #[test]
    fn test_scss_variable_with_default_flag() {
        // PrimeNG pattern: $variable: value !default;
        let symbols = parse_scss("$primary-color: blue !default;");
        let var = symbols.iter().find(|s| s.kind == SymbolKind::Variable);
        assert!(
            var.is_some(),
            "should extract $variable with !default as Variable, got: {:?}",
            symbols
        );
        assert_eq!(var.unwrap().name, "$primary-color");
    }

    #[test]
    fn test_scss_variable_inside_rule_set_extracted() {
        // PrimeNG pattern: $variable inside a :root {} block
        let symbols = parse_scss(":root {\n  $primary-color: blue;\n  --gap: 8px;\n}");
        let vars: Vec<&str> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .map(|s| s.name.as_str())
            .collect();
        assert!(
            vars.contains(&"$primary-color"),
            "should extract $variable inside :root block, got: {:?}",
            vars
        );
        assert!(
            vars.contains(&"--gap"),
            "should extract --custom-property inside :root block, got: {:?}",
            vars
        );
    }

    #[test]
    fn test_scss_variable_with_global_flag() {
        // SCSS !global flag — tree-sitter parses !global as ERROR node but variable still extracted
        let symbols = parse_scss("$primary-color: blue !global;");
        let var = symbols.iter().find(|s| s.kind == SymbolKind::Variable);
        assert!(
            var.is_some(),
            "should extract $variable with !global as Variable, got: {:?}",
            symbols
        );
        assert_eq!(var.unwrap().name, "$primary-color");
    }
}
