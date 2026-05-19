// TODO(SRTK04): Add inline_test! cases for the remaining language extractors in follow-up goals.
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
                $crate::parsing::parse_source(source, &language)
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
