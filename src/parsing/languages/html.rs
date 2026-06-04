use std::collections::HashSet;
use tree_sitter::Node;

use super::{NO_DOC_SPEC, SymbolSink, push_symbol};
use crate::domain::{SymbolKind, SymbolRecord};

pub fn extract_symbols(node: &Node, source: &str) -> Vec<SymbolRecord> {
    let mut symbols = Vec::new();
    let mut sort_order = 0u32;
    let mut emitted: HashSet<(u32, u32)> = HashSet::new();
    walk_node(node, source, 0, &mut sort_order, &mut symbols, &mut emitted);
    symbols
}

/// Two-tier extraction:
/// - AST-backed: elements (top-level, custom, ng-template), template refs
/// - Text-scanned: Angular control flow (@if/@for/@switch/@defer), @let, from `text` nodes
fn walk_node(
    node: &Node,
    source: &str,
    depth: u32,
    sort_order: &mut u32,
    symbols: &mut Vec<SymbolRecord>,
    emitted: &mut HashSet<(u32, u32)>,
) {
    match node.kind() {
        // tree-sitter-html wraps self-closing tags (e.g. <input />) inside
        // `element` nodes, so "self_closing_tag" is not a separate match arm.
        // extract_tag_name handles both start_tag and self_closing_tag children.
        "element" => {
            let tag_name = extract_tag_name(node, source);
            if let Some(ref name) = tag_name {
                let is_top_level = depth == 0;
                let is_custom = name.contains('-');
                let is_ng_template = name == "ng-template";

                if is_top_level || is_custom || is_ng_template {
                    let byte_key = (node.start_byte() as u32, node.end_byte() as u32);
                    if emitted.insert(byte_key) {
                        let mut sink = SymbolSink::new(source, sort_order, symbols, &NO_DOC_SPEC);
                        push_symbol(node, name.clone(), SymbolKind::Other, depth, &mut sink);
                    }
                }
            }
            // Scan attributes for template refs (#name)
            scan_template_refs(node, source, depth, sort_order, symbols, emitted);
            // Always recurse to find nested custom elements
            walk_children(node, source, depth + 1, sort_order, symbols, emitted);
        }
        "text" => {
            // Text-scanned: Angular control flow and @let declarations.
            // tree-sitter-html does not parse Angular syntax — these appear as raw text.
            let text = node.utf8_text(source.as_bytes()).unwrap_or("");
            scan_angular_text(node, source, text, depth, sort_order, symbols, emitted);
        }
        _ => {
            walk_children(node, source, depth, sort_order, symbols, emitted);
        }
    }
}

fn walk_children(
    node: &Node,
    source: &str,
    depth: u32,
    sort_order: &mut u32,
    symbols: &mut Vec<SymbolRecord>,
    emitted: &mut HashSet<(u32, u32)>,
) {
    let Some(_frame) = super::enter_ast_walk_frame() else {
        return;
    };
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_node(&child, source, depth, sort_order, symbols, emitted);
    }
}

/// Extract tag name from an element node.
/// For `element` nodes, the tag name lives inside a `start_tag` or `self_closing_tag` child.
fn extract_tag_name(node: &Node, source: &str) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "start_tag" || child.kind() == "self_closing_tag" {
            let mut inner_cursor = child.walk();
            for grandchild in child.children(&mut inner_cursor) {
                if grandchild.kind() == "tag_name" {
                    return Some(
                        grandchild
                            .utf8_text(source.as_bytes())
                            .unwrap_or("")
                            .to_string(),
                    );
                }
            }
        }
    }
    None
}

/// Scan start_tag / self_closing_tag attributes for template refs (#name).
fn scan_template_refs(
    node: &Node,
    source: &str,
    depth: u32,
    sort_order: &mut u32,
    symbols: &mut Vec<SymbolRecord>,
    emitted: &mut HashSet<(u32, u32)>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "start_tag" || child.kind() == "self_closing_tag" {
            let mut inner_cursor = child.walk();
            for attr in child.children(&mut inner_cursor) {
                if attr.kind() == "attribute" {
                    // Find attribute_name child
                    let mut attr_cursor = attr.walk();
                    for attr_child in attr.children(&mut attr_cursor) {
                        if attr_child.kind() == "attribute_name" {
                            let text = attr_child.utf8_text(source.as_bytes()).unwrap_or("");
                            if text.starts_with('#') && text.len() > 1 {
                                let ref_name = &text[1..];
                                let byte_key = (attr.start_byte() as u32, attr.end_byte() as u32);
                                if emitted.insert(byte_key) {
                                    let mut sink =
                                        SymbolSink::new(source, sort_order, symbols, &NO_DOC_SPEC);
                                    push_symbol(
                                        &attr,
                                        ref_name.to_string(),
                                        SymbolKind::Variable,
                                        depth,
                                        &mut sink,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Text-based scanning for Angular constructs in `text` nodes.
/// Detects: @if, @for, @switch, @defer (→ Module), @let name = ... (→ Variable).
/// Skips: @else, @empty (subordinate branches).
///
/// NOTE: Ranges for text-scanned symbols are coarse — anchored to the HTML text
/// node, not real Angular AST nodes. This is best-effort extraction; the Angular
/// grammar (tree-sitter-angular) was incompatible with the host tree-sitter 0.26
/// runtime, so Angular constructs are detected via line-by-line text scanning.
fn scan_angular_text(
    node: &Node,
    _source: &str,
    text: &str,
    depth: u32,
    sort_order: &mut u32,
    symbols: &mut Vec<SymbolRecord>,
    emitted: &mut HashSet<(u32, u32)>,
) {
    // A single text node may contain multiple Angular constructs.
    // Scan line by line to detect each one independently.
    //
    // We construct SymbolRecord directly (instead of calling push_symbol) so
    // that each construct gets its own per-line byte_range and line_range,
    // rather than sharing the enclosing text node's span.
    let node_start = node.start_byte() as u32;
    let start_line = node.start_position().row as u32;
    let mut offset = 0u32;
    for (line_idx, line) in text.split('\n').enumerate() {
        let line_start = node_start + offset;
        let trimmed = line.trim_start();

        // Control flow: @if, @for, @switch, @defer
        for keyword in &["@if", "@for", "@switch", "@defer"] {
            if let Some(rest) = trimmed.strip_prefix(keyword)
                && rest.starts_with([' ', '(', '{'])
            {
                let byte_key = (line_start, line_start + line.len() as u32);
                if emitted.insert(byte_key) {
                    let current_line = start_line + line_idx as u32;
                    symbols.push(SymbolRecord {
                        name: keyword.to_string(),
                        kind: SymbolKind::Module,
                        depth,
                        sort_order: *sort_order,
                        byte_range: byte_key,
                        line_range: (current_line, current_line),
                        doc_byte_range: None,
                        item_byte_range: Some(byte_key),
                    });
                    *sort_order += 1;
                }
                break; // Only one keyword per line
            }
        }

        // @let name = expr
        if let Some(rest) = trimmed.strip_prefix("@let ") {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
                .collect();
            if !name.is_empty() {
                let byte_key = (line_start, line_start + line.len() as u32);
                if emitted.insert(byte_key) {
                    let current_line = start_line + line_idx as u32;
                    symbols.push(SymbolRecord {
                        name,
                        kind: SymbolKind::Variable,
                        depth,
                        sort_order: *sort_order,
                        byte_range: byte_key,
                        line_range: (current_line, current_line),
                        doc_byte_range: None,
                        item_byte_range: Some(byte_key),
                    });
                    *sort_order += 1;
                }
            }
        }

        // @else, @empty — intentionally NOT extracted (subordinate branches)

        offset = offset.saturating_add(line.len() as u32 + 1); // +1 for the '\n'
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
        html_inline_test_extracts_custom_element,
        LanguageId::Html,
        r#"
<app-inline-probe></app-inline-probe>
"#,
        [(SymbolKind::Other, "app-inline-probe")]
    );

    fn parse_html(source: &str) -> Vec<SymbolRecord> {
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_html::LANGUAGE.into();
        parser.set_language(&lang).expect("set HTML language");
        let tree = parser.parse(source, None).expect("parse HTML source");
        extract_symbols(&tree.root_node(), source)
    }

    // ─── AST-backed: elements ──────────────────────────────────────────

    #[test]
    fn test_html_top_level_element() {
        let symbols = parse_html("<header>content</header>");
        let el = symbols.iter().find(|s| s.name == "header");
        assert!(
            el.is_some(),
            "should extract top-level element, got: {:?}",
            symbols
        );
        assert_eq!(el.unwrap().kind, SymbolKind::Other);
    }

    #[test]
    fn test_html_custom_element_any_depth() {
        let symbols = parse_html("<div><section><app-header></app-header></section></div>");
        let custom = symbols.iter().find(|s| s.name == "app-header");
        assert!(
            custom.is_some(),
            "should extract custom element at any depth, got: {:?}",
            symbols
        );
    }

    #[test]
    fn test_html_ng_template() {
        let symbols = parse_html("<ng-template>content</ng-template>");
        let tmpl = symbols.iter().find(|s| s.name == "ng-template");
        assert!(
            tmpl.is_some(),
            "should extract ng-template, got: {:?}",
            symbols
        );
    }

    #[test]
    fn test_html_generic_nested_skipped() {
        let symbols = parse_html("<div><p><span>text</span></p></div>");
        // Only top-level div should be extracted
        assert_eq!(
            symbols.len(),
            1,
            "only top-level element, got: {:?}",
            symbols
        );
        assert_eq!(symbols[0].name, "div");
    }

    #[test]
    fn test_html_top_level_custom_element_not_duped() {
        let symbols = parse_html("<app-root>content</app-root>");
        let matches: Vec<_> = symbols.iter().filter(|s| s.name == "app-root").collect();
        assert_eq!(
            matches.len(),
            1,
            "top-level custom element should appear once, got: {:?}",
            matches
        );
    }

    // ─── AST-backed: template refs ─────────────────────────────────────

    #[test]
    fn test_html_template_ref() {
        let symbols = parse_html("<input #myInput />");
        let tref = symbols
            .iter()
            .find(|s| s.name == "myInput" && s.kind == SymbolKind::Variable);
        assert!(
            tref.is_some(),
            "should extract template ref, got: {:?}",
            symbols
        );
    }

    // ─── Text-scanned: Angular control flow ────────────────────────────

    #[test]
    fn test_html_control_flow_if() {
        let symbols = parse_html("@if (condition) { <span>yes</span> }");
        let ctrl = symbols.iter().find(|s| s.name == "@if");
        assert!(ctrl.is_some(), "should extract @if, got: {:?}", symbols);
        assert_eq!(ctrl.unwrap().kind, SymbolKind::Module);
    }

    #[test]
    fn test_html_control_flow_for() {
        let symbols = parse_html("@for (item of items; track item.id) { <li>hi</li> }");
        let ctrl = symbols.iter().find(|s| s.name == "@for");
        assert!(ctrl.is_some(), "should extract @for, got: {:?}", symbols);
        assert_eq!(ctrl.unwrap().kind, SymbolKind::Module);
    }

    #[test]
    fn test_html_control_flow_switch() {
        let symbols = parse_html("@switch (value) { }");
        let ctrl = symbols.iter().find(|s| s.name == "@switch");
        assert!(ctrl.is_some(), "should extract @switch, got: {:?}", symbols);
    }

    #[test]
    fn test_html_control_flow_defer() {
        let symbols = parse_html("@defer (on viewport) { <app-heavy></app-heavy> }");
        let ctrl = symbols.iter().find(|s| s.name == "@defer");
        assert!(ctrl.is_some(), "should extract @defer, got: {:?}", symbols);
    }

    #[test]
    fn test_html_else_not_extracted() {
        let symbols = parse_html("@if (x) { <span>yes</span> } @else { <span>no</span> }");
        let else_sym = symbols.iter().find(|s| s.name.contains("@else"));
        assert!(
            else_sym.is_none(),
            "@else should NOT be a separate symbol, got: {:?}",
            symbols
        );
    }

    #[test]
    fn test_html_empty_not_extracted() {
        let symbols = parse_html(
            "@for (item of items; track item.id) { <p>x</p> } @empty { <p>No items</p> }",
        );
        let empty_sym = symbols.iter().find(|s| s.name.contains("@empty"));
        assert!(
            empty_sym.is_none(),
            "@empty should NOT be a separate symbol, got: {:?}",
            symbols
        );
    }

    // ─── Text-scanned: @let declarations ───────────────────────────────

    #[test]
    fn test_html_let_declaration() {
        let symbols = parse_html("@let user = currentUser();");
        let letvar = symbols
            .iter()
            .find(|s| s.name == "user" && s.kind == SymbolKind::Variable);
        assert!(letvar.is_some(), "should extract @let, got: {:?}", symbols);
    }

    // ─── Regression: plain HTML ────────────────────────────────────────

    #[test]
    fn test_html_plain_no_angular_noise() {
        let symbols = parse_html("<div><p>text</p></div>");
        let div = symbols.iter().find(|s| s.name == "div");
        assert!(div.is_some(), "should extract top-level div");
        let angular_noise: Vec<_> = symbols.iter().filter(|s| s.name.starts_with('@')).collect();
        assert!(
            angular_noise.is_empty(),
            "plain HTML should have no Angular noise, got: {:?}",
            angular_noise
        );
    }

    #[test]
    fn test_html_empty_file() {
        let symbols = parse_html("");
        assert!(symbols.is_empty(), "empty file should produce zero symbols");
    }

    #[test]
    fn test_html_realistic_angular_template() {
        // Realistic Angular template: @if/@for nested inside a real HTML element.
        // This exercises the code path where Angular control flow text nodes are
        // children of an `element` node (not top-level document children).
        let source = r#"<div class="container">
  <h1>Title</h1>
  @if (isLoggedIn) {
    <app-dashboard></app-dashboard>
  } @else {
    <app-login></app-login>
  }
  @for (item of items; track item.id) {
    <app-item [data]="item"></app-item>
  }
</div>"#;
        let symbols = parse_html(source);
        let if_sym = symbols.iter().find(|s| s.name == "@if");
        let for_sym = symbols.iter().find(|s| s.name == "@for");
        assert!(
            if_sym.is_some(),
            "should extract @if from realistic template, got: {:?}",
            symbols
        );
        assert!(
            for_sym.is_some(),
            "should extract @for from realistic template, got: {:?}",
            symbols
        );
    }

    #[test]
    fn test_html_debug_node_kinds_realistic_angular() {
        // Diagnostic: print the tree-sitter parse tree for a realistic Angular template
        // so we know which node kinds contain @if/@for text.
        let source = r#"<div class="container">
  <h1>Title</h1>
  @if (isLoggedIn) {
    <app-dashboard></app-dashboard>
  } @else {
    <app-login></app-login>
  }
  @for (item of items; track item.id) {
    <app-item [data]="item"></app-item>
  }
</div>"#;
        let mut parser = tree_sitter::Parser::new();
        let lang: tree_sitter::Language = tree_sitter_html::LANGUAGE.into();
        parser.set_language(&lang).expect("set HTML language");
        let tree = parser.parse(source, None).expect("parse");

        fn dump_node(node: &tree_sitter::Node, source: &str, indent: usize) {
            let text = node
                .utf8_text(source.as_bytes())
                .unwrap_or("")
                .replace('\n', "\\n");
            let preview = if text.len() > 60 { &text[..60] } else { &text };
            eprintln!(
                "{:indent$}[{}] {:?} ({}-{})",
                "",
                node.kind(),
                preview,
                node.start_byte(),
                node.end_byte(),
                indent = indent
            );
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                dump_node(&child, source, indent + 2);
            }
        }

        dump_node(&tree.root_node(), source, 0);
        // This test always passes — its purpose is to emit the parse tree to stderr.
        // Run with: cargo test test_html_debug_node_kinds -- --nocapture 2>&1
    }

    // ─── Regression: multiple Angular constructs in one text node ─────

    #[test]
    fn test_html_multiple_angular_constructs_in_one_text_node() {
        // tree-sitter-html may merge adjacent Angular lines into one text node
        let symbols = parse_html(
            "@if (a) { }\n@for (item of items; track item.id) { }\n@let user = currentUser();",
        );
        let if_sym = symbols.iter().find(|s| s.name == "@if");
        let for_sym = symbols.iter().find(|s| s.name == "@for");
        let let_sym = symbols.iter().find(|s| s.name == "user");
        assert!(if_sym.is_some(), "should find @if, got: {:?}", symbols);
        assert!(for_sym.is_some(), "should find @for, got: {:?}", symbols);
        assert!(
            let_sym.is_some(),
            "should find @let user, got: {:?}",
            symbols
        );
    }

    // ─── Regression: malformed Angular-ish text ────────────────────────

    #[test]
    fn test_html_malformed_angular_no_panic() {
        // Should not panic or produce runaway ranges
        let symbols = parse_html("@ @if @let @for( @switch");
        // We don't assert specific output — just no panic
        let _ = symbols;
    }

    // ─── SF-004: Angular control-flow `>` trips tree-sitter-html ────────

    #[test]
    fn test_sf004_angular_if_relational_operator_is_partial_but_extracts_symbol() {
        // tree-sitter-html 0.23.2 has zero Angular rules; the `>` relational
        // operator in `@if (items.length > 0) {` is lexed as a tag close, so the
        // grammar reports a parse error even though SymForge text-scans the
        // construct and still extracts the `@if` symbol. This pins BOTH halves of
        // the SF-004 root cause: has_error=true AND the symbol survives.
        let source = "<div>\n  @if (items.length > 0) {\n    <span>{{ items.length }}</span>\n  }\n</div>";
        let (symbols, has_error, _diagnostic, _refs, _aliases) =
            crate::parsing::parse_source(source, &LanguageId::Html)
                .expect("HTML parse should not hard-fail");

        assert!(
            has_error,
            "the `>` inside an Angular @if expression must trip tree-sitter-html into an error node"
        );
        let if_sym = symbols
            .iter()
            .find(|s| s.name == "@if" && s.kind == SymbolKind::Module);
        assert!(
            if_sym.is_some(),
            "the @if control-flow symbol must still be text-scanned despite the parse error, got: {:?}",
            symbols
        );
    }
}
