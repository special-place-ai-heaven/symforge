macro_rules! inline_test {
    (
        $name:ident,
        $language:expr,
        $source:expr,
        [$(($kind:expr, $symbol_name:expr)),* $(,)?]
    ) => {
        #[test]
        fn $name() {
            let language: $crate::domain::LanguageId = $language;
            let source: &str = $source;
            let (symbols, has_error, diagnostic, _, _) =
                $crate::parsing::parse_source(source, &language, false)
                    .expect("inline language test source should parse");

            assert!(
                !has_error,
                "inline language test for {language} reported parse errors: {diagnostic:?}"
            );

            let actual: Vec<($crate::domain::SymbolKind, &str)> = symbols
                .iter()
                .map(|symbol| (symbol.kind, symbol.name.as_str()))
                .collect();
            let expected: Vec<($crate::domain::SymbolKind, &str)> = vec![
                $(($kind, $symbol_name)),*
            ];

            assert_eq!(actual, expected, "symbols extracted for {language}");
        }
    };
}

pub(crate) use inline_test;

#[cfg(test)]
mod systems_backend_tests {
    use crate::domain::{LanguageId, SymbolKind};

    inline_test!(
        go_inline_test_extracts_function,
        LanguageId::Go,
        r#"
package main

func InlineGoProbe() {}
"#,
        [(SymbolKind::Function, "InlineGoProbe")]
    );

    inline_test!(
        java_inline_test_extracts_class,
        LanguageId::Java,
        r#"
public class InlineJavaProbe {}
"#,
        [(SymbolKind::Class, "InlineJavaProbe")]
    );

    inline_test!(
        c_inline_test_extracts_function,
        LanguageId::C,
        r#"
int inline_c_probe(void) { return 0; }
"#,
        [(SymbolKind::Function, "inline_c_probe")]
    );

    inline_test!(
        cpp_inline_test_extracts_function,
        LanguageId::Cpp,
        r#"
int inline_cpp_probe() { return 0; }
"#,
        [(SymbolKind::Function, "inline_cpp_probe")]
    );

    inline_test!(
        csharp_inline_test_extracts_class,
        LanguageId::CSharp,
        r#"
public class InlineCSharpProbe {}
"#,
        [(SymbolKind::Class, "InlineCSharpProbe")]
    );

    inline_test!(
        swift_inline_test_extracts_class,
        LanguageId::Swift,
        r#"
class InlineSwiftProbe {}
"#,
        [(SymbolKind::Class, "InlineSwiftProbe")]
    );
}

#[cfg(test)]
mod scripting_and_remaining_tests {
    use crate::domain::{LanguageId, SymbolKind};

    inline_test!(
        ruby_inline_test_extracts_method,
        LanguageId::Ruby,
        r#"
def inline_ruby_probe
end
"#,
        [(SymbolKind::Method, "inline_ruby_probe")]
    );

    inline_test!(
        php_inline_test_extracts_function,
        LanguageId::Php,
        r#"
<?php
function inline_php_probe() {}
"#,
        [(SymbolKind::Function, "inline_php_probe")]
    );

    inline_test!(
        perl_inline_test_extracts_function,
        LanguageId::Perl,
        r#"
sub inline_perl_probe { return 1; }
"#,
        [(SymbolKind::Function, "inline_perl_probe")]
    );

    inline_test!(
        kotlin_inline_test_extracts_function,
        LanguageId::Kotlin,
        r#"
fun inlineKotlinProbe() = Unit
"#,
        [(SymbolKind::Function, "inlineKotlinProbe")]
    );

    inline_test!(
        dart_inline_test_extracts_function,
        LanguageId::Dart,
        r#"
void inlineDartProbe() {}
"#,
        [(SymbolKind::Function, "inlineDartProbe")]
    );

    inline_test!(
        elixir_inline_test_extracts_function,
        LanguageId::Elixir,
        r#"
def inline_elixir_probe do
  :ok
end
"#,
        [(SymbolKind::Function, "inline_elixir_probe")]
    );
}
