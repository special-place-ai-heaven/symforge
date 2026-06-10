//! Adapter between SymForge's `LanguageId` and `ast_grep_core` types.
//!
//! Implements `ast_grep_core::Language` and `LanguageExt` so we can use
//! ast-grep's structural pattern matching on indexed source files.
//!
//! Key detail: many languages don't accept `$` as an identifier character.
//! We must replace `$` with a Unicode expando char before parsing so
//! tree-sitter treats metavariables as valid identifiers. The expando chars
//! match the official `ast-grep-language` crate exactly.

use crate::domain::index::LanguageId;
use ast_grep_core::language::Language;
use ast_grep_core::matcher::PatternBuilder;
use ast_grep_core::meta_var::MetaVariable;
use ast_grep_core::tree_sitter::{LanguageExt, StrDoc, TSLanguage};
use ast_grep_core::{Pattern, PatternError};
use std::borrow::Cow;

/// Wrapper that implements ast-grep's `Language` trait for a SymForge `LanguageId`.
#[derive(Clone)]
pub struct SgLang {
    ts_lang: TSLanguage,
    /// The character used to replace `$` before parsing. Languages that accept
    /// `$` in identifiers (JavaScript, TypeScript, Java, Dart) use `$` directly.
    /// Others use a Unicode letter that the grammar accepts as an identifier.
    expando: char,
}

/// Pre-process a pattern string by replacing `$` metavariable sigils with the
/// language-specific expando character. This is the same algorithm used by the
/// official `ast-grep-language` crate.
fn pre_process_pattern(expando: char, query: &str) -> Cow<'_, str> {
    if expando == '$' {
        return Cow::Borrowed(query);
    }
    let mut ret = Vec::with_capacity(query.len());
    let mut dollar_count = 0usize;
    for c in query.chars() {
        if c == '$' {
            dollar_count += 1;
            continue;
        }
        let need_replace = matches!(c, 'A'..='Z' | '_') || dollar_count == 3;
        let sigil = if need_replace { expando } else { '$' };
        ret.extend(std::iter::repeat_n(sigil, dollar_count));
        dollar_count = 0;
        ret.push(c);
    }
    // Trailing anonymous multiple ($$$)
    let sigil = if dollar_count == 3 { expando } else { '$' };
    ret.extend(std::iter::repeat_n(sigil, dollar_count));
    Cow::Owned(ret.into_iter().collect())
}

impl SgLang {
    /// Returns `None` for config-only languages (JSON, TOML, YAML, Markdown, Env)
    /// that have no meaningful AST patterns.
    pub fn from_language_id(lang: &LanguageId, is_tsx: bool) -> Option<Self> {
        // Expando chars sourced from the official ast-grep-language crate.
        // Languages that accept `$` as an identifier char use '$' (no replacement).
        // Languages that don't accept `$` use a Unicode letter the grammar allows.
        let (ts_lang, expando): (TSLanguage, char) = match lang {
            // Expando languages (don't accept $ in identifiers)
            LanguageId::Rust => (tree_sitter_rust::LANGUAGE.into(), 'µ'),
            LanguageId::Python => (tree_sitter_python::LANGUAGE.into(), 'µ'),
            LanguageId::Go => (tree_sitter_go::LANGUAGE.into(), 'µ'),
            LanguageId::C => (tree_sitter_c::LANGUAGE.into(), '𐀀'),
            LanguageId::Cpp => (tree_sitter_cpp::LANGUAGE.into(), '𐀀'),
            LanguageId::CSharp => (tree_sitter_c_sharp::LANGUAGE.into(), 'µ'),
            LanguageId::Ruby => (tree_sitter_ruby::LANGUAGE.into(), 'µ'),
            LanguageId::Php => (tree_sitter_php::LANGUAGE_PHP.into(), 'µ'),
            LanguageId::Swift => (tree_sitter_swift::LANGUAGE.into(), 'µ'),
            LanguageId::Kotlin => (tree_sitter_kotlin_sg::LANGUAGE.into(), 'µ'),
            LanguageId::Elixir => (tree_sitter_elixir::LANGUAGE.into(), 'µ'),
            LanguageId::Css => (tree_sitter_css::LANGUAGE.into(), '_'),
            LanguageId::Scss => (tree_sitter_scss::language(), '_'),
            // Stub languages (accept $ in identifiers — no expando needed)
            LanguageId::JavaScript => (tree_sitter_javascript::LANGUAGE.into(), '$'),
            // `.tsx` uses the JSX-aware TSX grammar so structural patterns can
            // match inside JSX; `.ts` stays on the plain TypeScript grammar.
            LanguageId::TypeScript if is_tsx => (tree_sitter_typescript::LANGUAGE_TSX.into(), '$'),
            LanguageId::TypeScript => (tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(), '$'),
            LanguageId::Java => (tree_sitter_java::LANGUAGE.into(), '$'),
            LanguageId::Dart => (tree_sitter_dart::language(), '$'),
            LanguageId::Perl => (tree_sitter_perl::LANGUAGE.into(), '$'),
            LanguageId::Html => (tree_sitter_html::LANGUAGE.into(), '$'),
            // Config-only languages — no structural patterns
            LanguageId::Json
            | LanguageId::Toml
            | LanguageId::Yaml
            | LanguageId::Markdown
            | LanguageId::Env => return None,
        };
        Some(Self { ts_lang, expando })
    }
}

impl Language for SgLang {
    fn kind_to_id(&self, kind: &str) -> u16 {
        self.ts_lang.id_for_node_kind(kind, true)
    }

    fn field_to_id(&self, field: &str) -> Option<u16> {
        self.ts_lang.field_id_for_name(field).map(|f| f.get())
    }

    fn expando_char(&self) -> char {
        self.expando
    }

    fn pre_process_pattern<'q>(&self, query: &'q str) -> Cow<'q, str> {
        pre_process_pattern(self.expando, query)
    }

    fn build_pattern(&self, builder: &PatternBuilder) -> Result<Pattern, PatternError> {
        builder.build(|src| StrDoc::try_new(src, self.clone()))
    }
}

impl LanguageExt for SgLang {
    fn get_ts_language(&self) -> TSLanguage {
        self.ts_lang.clone()
    }
}

/// A single structural match result.
pub struct StructuralMatch {
    /// Byte offset of the match start in the source.
    pub start_byte: usize,
    /// Byte offset of the match end in the source.
    pub end_byte: usize,
    /// Zero-based start line.
    pub start_line: usize,
    /// Zero-based start column.
    pub start_col: usize,
    /// The matched text.
    pub text: String,
    /// Captured metavariables: (name, text).
    pub captures: Vec<(String, String)>,
}

/// A structural pattern compiled for one SymForge language.
pub struct CompiledStructuralPattern {
    lang: SgLang,
    pattern: Pattern,
}

/// Compile an ast-grep pattern for the given language.
///
/// Returns an error string if the language is unsupported or the pattern cannot be compiled.
pub fn compile_structural_pattern(
    pattern_str: &str,
    lang: &LanguageId,
    is_tsx: bool,
) -> Result<CompiledStructuralPattern, String> {
    let sg_lang = SgLang::from_language_id(lang, is_tsx)
        .ok_or_else(|| format!("structural search not supported for {:?}", lang))?;

    let pattern = Pattern::try_new(pattern_str, sg_lang.clone())
        .map_err(|e| format!("invalid structural pattern: {e}"))?;

    Ok(CompiledStructuralPattern {
        lang: sg_lang,
        pattern,
    })
}

/// Search `source` using an already compiled ast-grep pattern.
pub fn structural_search_with_compiled(
    source: &str,
    compiled: &CompiledStructuralPattern,
) -> Vec<StructuralMatch> {
    let root = compiled.lang.ast_grep(source);

    root.root()
        .find_all(&compiled.pattern)
        .map(|node_match| {
            let start = node_match.start_pos();
            let text = node_match.text().to_string();

            // Extract metavariable captures
            let env = node_match.get_env();
            let captures: Vec<(String, String)> = env
                .get_matched_variables()
                .filter_map(|var| match var {
                    MetaVariable::Capture(name, _) => {
                        env.get_match(&name).map(|n| (name, n.text().to_string()))
                    }
                    MetaVariable::MultiCapture(name) => {
                        let nodes = env.get_multiple_matches(&name);
                        let combined: String = nodes
                            .iter()
                            .map(|n| n.text().to_string())
                            .collect::<Vec<_>>()
                            .join(", ");
                        if combined.is_empty() {
                            None
                        } else {
                            Some((name, combined))
                        }
                    }
                    _ => None,
                })
                .collect();

            StructuralMatch {
                start_byte: node_match.range().start,
                end_byte: node_match.range().end,
                start_line: start.line(),
                start_col: start.byte_point().1,
                text,
                captures,
            }
        })
        .collect()
}

/// Search `source` for occurrences of an ast-grep `pattern` in the given language.
///
/// Returns an error string if the pattern cannot be compiled (e.g., syntax error).
pub fn structural_search(
    source: &str,
    pattern_str: &str,
    lang: &LanguageId,
    is_tsx: bool,
) -> Result<Vec<StructuralMatch>, String> {
    let compiled = compile_structural_pattern(pattern_str, lang, is_tsx)?;
    Ok(structural_search_with_compiled(source, &compiled))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_structural_search_rust_function() {
        let source = r#"
fn hello() {
    println!("hello");
}
fn world(x: i32) {
    println!("{}", x);
}
"#;
        let matches = structural_search(source, "fn $NAME($$$) { $$$ }", &LanguageId::Rust, false)
            .expect("pattern should compile");
        assert_eq!(matches.len(), 2);
        assert!(matches[0].text.contains("hello"));
        assert!(matches[1].text.contains("world"));
    }

    #[test]
    fn test_structural_search_captures() {
        let source = "let x = 42;\nlet y = 100;";
        let matches = structural_search(source, "let $NAME = $VALUE", &LanguageId::Rust, false)
            .expect("pattern should compile");
        assert_eq!(matches.len(), 2);
        assert!(!matches[0].captures.is_empty());
    }

    #[test]
    fn test_structural_search_no_match() {
        let source = "fn main() {}";
        let matches = structural_search(
            source,
            "struct $NAME { $$$FIELDS }",
            &LanguageId::Rust,
            false,
        )
        .expect("pattern should compile");
        assert!(matches.is_empty());
    }

    #[test]
    fn test_structural_search_config_language_rejected() {
        let result = structural_search("{}", "{ $$$BODY }", &LanguageId::Json, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_structural_search_javascript() {
        let source = "const x = 42;\nconst y = 100;";
        let matches = structural_search(
            source,
            "const $NAME = $VALUE",
            &LanguageId::JavaScript,
            false,
        )
        .expect("pattern should compile");
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_structural_search_rust_struct() {
        let source = r#"
pub struct Foo {
    pub name: String,
    pub age: u32,
}
struct Bar {
    x: i32,
}
"#;
        // `struct $NAME` matches bare structs; `pub struct` needs explicit `pub` in pattern
        let bare = structural_search(
            source,
            "struct $NAME { $$$FIELDS }",
            &LanguageId::Rust,
            false,
        )
        .expect("pattern should compile");
        assert_eq!(bare.len(), 1, "bare struct pattern matches Bar");
        assert!(bare[0].text.contains("Bar"));

        let pub_matches = structural_search(
            source,
            "pub struct $NAME { $$$FIELDS }",
            &LanguageId::Rust,
            false,
        )
        .expect("pattern should compile");
        assert_eq!(pub_matches.len(), 1, "pub struct pattern matches Foo");
        assert!(pub_matches[0].text.contains("Foo"));
    }

    #[test]
    fn test_structural_search_rust_impl() {
        let source = "impl Foo { fn bar(&self) {} }";
        let matches = structural_search(source, "impl $TYPE { $$$ }", &LanguageId::Rust, false)
            .expect("pattern should compile");
        assert_eq!(matches.len(), 1);
        assert!(matches[0].text.contains("Foo"));
    }

    #[test]
    fn test_structural_search_python_function() {
        let source = "def greet(name):\n    print(name)\n";
        let matches = structural_search(
            source,
            "def $FNAME($$$):\n    $$$",
            &LanguageId::Python,
            false,
        )
        .expect("pattern should compile");
        assert!(!matches.is_empty(), "should match Python function def");
    }

    #[test]
    fn test_structural_search_go_function() {
        let source = "func hello() { fmt.Println(\"hello\") }";
        let matches = structural_search(source, "func $NAME() { $$$ }", &LanguageId::Go, false)
            .expect("pattern should compile");
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn test_structural_search_tsx_jsx_component() {
        // A JSX component body only parses under the TSX grammar. With the plain
        // TypeScript grammar this yields a partial parse and the structural
        // pattern matches nothing; with the TSX grammar (is_tsx=true) the
        // function declaration is matched.
        let source = r#"
export function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>
  );
}
"#;
        let matches = structural_search(
            source,
            "function $NAME() { $$$ }",
            &LanguageId::TypeScript,
            true,
        )
        .expect("pattern should compile against the TSX grammar");
        assert_eq!(
            matches.len(),
            1,
            "TSX grammar must match the JSX component function"
        );
        assert!(matches[0].text.contains("App"));
    }
}
