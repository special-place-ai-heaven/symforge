use super::*;

fn check(source: &str, language: LanguageId, blocked: &[&str], exact: &[(&str, &str)]) {
    let tree = crate::parsing::parse_source_tree(source, &language, false).expect("parse fixture");
    let root = tree.root_node();
    assert!(!root.has_error(), "invalid fixture: {}", root.to_sexp());
    let facts = facts(root, source, language);
    let mut refs = Vec::new();
    extract(root, source, language, false, &mut refs);

    let use_range = |usage: &str| {
        let start = source.find(usage).expect("fixture contains callback") as u32;
        assert_eq!(
            source.matches(usage).count(),
            1,
            "ambiguous fixture: {usage}"
        );
        let end = start
            + usage
                .rfind([')', '}'])
                .expect("callback argument ends in ) or }") as u32;
        let leaf = usage[..(end - start) as usize]
            .rsplit(['.', '(', '{'])
            .next()
            .unwrap();
        (end - leaf.len() as u32, end)
    };
    for usage in blocked {
        let range = use_range(usage);
        assert!(!facts.locals.contains_key(&range), "exact fact for {usage}");
        assert!(
            !refs.iter().any(|reference| reference.byte_range == range),
            "value use for {usage}"
        );
    }
    for &(usage, declaration) in exact {
        let range = use_range(usage);
        let start = source
            .find(declaration)
            .expect("fixture contains declaration") as u32;
        let end = start + declaration.len() as u32;
        assert_eq!(facts.locals.get(&range), Some(&(start, end)), "{usage}");
        assert!(
            refs.iter().any(|reference| reference.byte_range == range
                && reference.kind == ReferenceKind::ValueUse),
            "missing value use for {usage}"
        );
    }
}

#[test]
fn javascript_receiver_writes_do_not_become_exact_method_callbacks() {
    check(
        r#"function handle() {}
class Worker {
    handle() {}
    bracket() {}
    fromConstructor() {}
    stable() {}
    constructor() { this.fromConstructor = () => {}; }
    static bindStatic() { this.stable = () => {}; }
    bind(other) {
        other.stable = () => {};
        this.handle = () => {};
        this["bracket"] = () => {};
        install(this.bracket);
        install(this.handle);
        install(this.fromConstructor);
        install(this.stable);
        install(handle);
    }
    another() { installAgain(this.handle); }
}
class Other {
    handle() { return 1; }
    bind() { installOther(this.handle); }
}"#,
        LanguageId::JavaScript,
        &[
            "install(this.handle)",
            "install(this.fromConstructor)",
            "install(this.bracket)",
            "installAgain(this.handle)",
        ],
        &[
            ("install(this.stable)", "stable() {}"),
            ("install(handle)", "function handle() {}"),
            ("installOther(this.handle)", "handle() { return 1; }"),
        ],
    );
}

#[test]
fn typescript_receiver_writes_in_arrows_and_patterns_block_exact_callbacks() {
    check(
        r#"class Worker {
    handle(): void {}
    stable(): void {}
    bind(other: Worker, replacement: () => void): void {
        const replace = () => { [this['handle']] = [replacement]; };
        replace();
        other.stable = replacement;
        install(this.handle);
        install(this.stable);
    }
}"#,
        LanguageId::TypeScript,
        &["install(this.handle)"],
        &[("install(this.stable)", "stable(): void {}")],
    );
}

#[test]
fn python_receiver_writes_block_exact_self_and_cls_callbacks() {
    check(
        r#"class Worker:
    def handle(self):
        pass

    @classmethod
    def configure(cls):
        pass

    @classmethod
    def stable_class(cls):
        pass

    def stable(self):
        pass

    def __init__(self):
        self.handle = lambda: None

    def bind(self, other):
        other.stable = lambda: None
        install(self.handle)
        install(self.stable)

    @classmethod
    def bind_class(cls, replacement):
        cls.configure = replacement
        install(cls.configure)
        install(cls.stable_class)

class Other:
    def handle(self):
        return 1

    def bind(self):
        install_other(self.handle)
"#,
        LanguageId::Python,
        &["install(self.handle)", "install(cls.configure)"],
        &[
            ("install(self.stable)", "def stable(self):\n        pass"),
            (
                "install(cls.stable_class)",
                "@classmethod\n    def stable_class(cls):\n        pass",
            ),
            (
                "install_other(self.handle)",
                "def handle(self):\n        return 1",
            ),
        ],
    );
}

#[test]
fn javascript_computed_receiver_writes_compare_decoded_keys() {
    for language in [LanguageId::JavaScript, LanguageId::TypeScript] {
        check(
            r#"class Unicode {
    handle() {}
    stable() { return 0; }
    bind(fn) {
        this["h\u0061ndle"] = fn;
        installUnicode(this.handle);
        installUnicodeStable(this.stable);
    }
}
class Hex {
    handle() {}
    stable() { return 1; }
    bind(fn) {
        this['h\x61ndle'] = fn;
        installHex(this.handle);
        installHexStable(this.stable);
    }
}
class Braced {
    handle() {}
    stable() { return 2; }
    bind(fn) {
        this["h\u{61}ndle"] = fn;
        installBraced(this.handle);
        installBracedStable(this.stable);
    }
}
class Template {
    handle() {}
    stable() { return 3; }
    bind(fn) {
        this[`h\u0061ndle`] = fn;
        installTemplate(this.handle);
        installTemplateStable(this.stable);
    }
}
class Plain {
    handle() {}
    stable() { return 4; }
    bind(fn) {
        this[`handle`] = fn;
        installPlain(this.handle);
        installPlainStable(this.stable);
    }
}
class Opaque {
    handle() {}
    stable() { return 5; }
    bind(key, fn) {
        this[key] = fn;
        installOpaque(this.handle);
        installOpaqueStable(this.stable);
    }
}
class Interpolated {
    handle() {}
    stable() { return 6; }
    bind(key, fn) {
        this[`${key}`] = fn;
        installInterpolated(this.handle);
        installInterpolatedStable(this.stable);
    }
}
"#,
            language,
            &[
                "installUnicode(this.handle)",
                "installHex(this.handle)",
                "installBraced(this.handle)",
                "installTemplate(this.handle)",
                "installPlain(this.handle)",
                "installOpaque(this.handle)",
                "installOpaqueStable(this.stable)",
                "installInterpolated(this.handle)",
                "installInterpolatedStable(this.stable)",
            ],
            &[
                (
                    "installUnicodeStable(this.stable)",
                    "stable() { return 0; }",
                ),
                ("installHexStable(this.stable)", "stable() { return 1; }"),
                ("installBracedStable(this.stable)", "stable() { return 2; }"),
                (
                    "installTemplateStable(this.stable)",
                    "stable() { return 3; }",
                ),
                ("installPlainStable(this.stable)", "stable() { return 4; }"),
            ],
        );
    }
}

#[test]
fn class_fields_and_escaped_names_block_exact_method_callbacks() {
    check(
        r#"class FieldArrow {
    handle() { return 0; }
    stable() { return 0; }
    swap = (fn) => { this.handle = fn; };
    bind() {
        installFieldArrow(this.handle);
        installFieldArrowStable(this.stable);
    }
}
class FieldBracket {
    handle() { return 1; }
    stable() { return 1; }
    swap = (fn) => { this["handle"] = fn; };
    bind() {
        installFieldBracket(this.handle);
        installFieldBracketStable(this.stable);
    }
}
class FieldOpaque {
    handle() { return 2; }
    stable() { return 2; }
    swap = (key, fn) => { this[key] = fn; };
    bind() {
        installFieldOpaque(this.handle);
        installFieldOpaqueStable(this.stable);
    }
}
class FieldValue {
    handle() { return 3; }
    stable() { return 3; }
    swap = this.handle = null;
    bind() {
        installFieldValue(this.handle);
        installFieldValueStable(this.stable);
    }
}
class StaticField {
    handle() { return 4; }
    stable() { return 4; }
    static swap = (fn) => { this.handle = fn; };
    bind() {
        installStaticField(this.handle);
        installStaticFieldStable(this.stable);
    }
}
class Shadow {
    handle() { return 5; }
    stable() { return 5; }
    handle = () => 0;
    bind() {
        installShadow(this.handle);
        installShadowStable(this.stable);
    }
}
class ShadowBracket {
    handle() { return 6; }
    stable() { return 6; }
    ["h\u0061ndle"] = () => 0;
    bind() {
        installShadowBracket(this.handle);
        installShadowBracketStable(this.stable);
    }
}
class EscapedWrite {
    handle() { return 7; }
    stable() { return 7; }
    swap(fn) { this.h\u0061ndle = fn; }
    bind() {
        installEscapedWrite(this.handle);
        installEscapedWriteStable(this.stable);
    }
}
class EscapedMethod {
    h\u0061ndle() { return 8; }
    stable() { return 8; }
    swap(fn) { this["handle"] = fn; }
    bind() {
        installEscapedMethod(this.h\u0061ndle);
        installEscapedMethodStable(this.stable);
    }
}
"#,
        LanguageId::JavaScript,
        &[
            "installFieldArrow(this.handle)",
            "installFieldBracket(this.handle)",
            "installFieldOpaque(this.handle)",
            "installFieldOpaqueStable(this.stable)",
            "installFieldValue(this.handle)",
            "installShadow(this.handle)",
            "installShadowBracket(this.handle)",
            "installEscapedWrite(this.handle)",
            "installEscapedMethod(this.h\\u0061ndle)",
        ],
        &[
            (
                "installFieldArrowStable(this.stable)",
                "stable() { return 0; }",
            ),
            (
                "installFieldBracketStable(this.stable)",
                "stable() { return 1; }",
            ),
            (
                "installFieldValueStable(this.stable)",
                "stable() { return 3; }",
            ),
            ("installStaticField(this.handle)", "handle() { return 4; }"),
            (
                "installStaticFieldStable(this.stable)",
                "stable() { return 4; }",
            ),
            ("installShadowStable(this.stable)", "stable() { return 5; }"),
            (
                "installShadowBracketStable(this.stable)",
                "stable() { return 6; }",
            ),
            (
                "installEscapedWriteStable(this.stable)",
                "stable() { return 7; }",
            ),
            (
                "installEscapedMethodStable(this.stable)",
                "stable() { return 8; }",
            ),
        ],
    );
    check(
        r#"class FieldArrow {
    handle() { return 0; }
    stable() { return 0; }
    swap = (fn) => { this.handle = fn; };
    bind() {
        installFieldArrow(this.handle);
        installFieldArrowStable(this.stable);
    }
}
class FieldBracket {
    handle() { return 1; }
    stable() { return 1; }
    swap = (fn) => { this["handle"] = fn; };
    bind() {
        installFieldBracket(this.handle);
        installFieldBracketStable(this.stable);
    }
}
class FieldOpaque {
    handle() { return 2; }
    stable() { return 2; }
    swap = (key, fn) => { this[key] = fn; };
    bind() {
        installFieldOpaque(this.handle);
        installFieldOpaqueStable(this.stable);
    }
}
class FieldValue {
    handle() { return 3; }
    stable() { return 3; }
    swap = this.handle = null;
    bind() {
        installFieldValue(this.handle);
        installFieldValueStable(this.stable);
    }
}
class StaticField {
    handle() { return 4; }
    stable() { return 4; }
    static swap = (fn) => { this.handle = fn; };
    bind() {
        installStaticField(this.handle);
        installStaticFieldStable(this.stable);
    }
}
class Shadow {
    handle() { return 5; }
    stable() { return 5; }
    handle = () => 0;
    bind() {
        installShadow(this.handle);
        installShadowStable(this.stable);
    }
}
class ShadowBracket {
    handle() { return 6; }
    stable() { return 6; }
    ["h\u0061ndle"] = () => 0;
    bind() {
        installShadowBracket(this.handle);
        installShadowBracketStable(this.stable);
    }
}
class EscapedWrite {
    handle() { return 7; }
    stable() { return 7; }
    swap(fn) { this.h\u0061ndle = fn; }
    bind() {
        installEscapedWrite(this.handle);
        installEscapedWriteStable(this.stable);
    }
}
class EscapedMethod {
    h\u0061ndle() { return 8; }
    stable() { return 8; }
    swap(fn) { this["handle"] = fn; }
    bind() {
        installEscapedMethod(this.h\u0061ndle);
        installEscapedMethodStable(this.stable);
    }
}
class Declared {
    handle() { return 9; }
    stable() { return 9; }
    declare handle: () => void;
    bind() {
        installDeclared(this.handle);
        installDeclaredStable(this.stable);
    }
}
class Definite {
    handle() { return 10; }
    stable() { return 10; }
    handle!: () => void;
    bind() {
        installDefinite(this.handle);
        installDefiniteStable(this.stable);
    }
}
"#,
        LanguageId::TypeScript,
        &[
            "installFieldArrow(this.handle)",
            "installFieldBracket(this.handle)",
            "installFieldOpaque(this.handle)",
            "installFieldOpaqueStable(this.stable)",
            "installFieldValue(this.handle)",
            "installShadow(this.handle)",
            "installShadowBracket(this.handle)",
            "installEscapedWrite(this.handle)",
            "installEscapedMethod(this.h\\u0061ndle)",
            "installDeclared(this.handle)",
            "installDefinite(this.handle)",
        ],
        &[
            (
                "installFieldArrowStable(this.stable)",
                "stable() { return 0; }",
            ),
            (
                "installFieldBracketStable(this.stable)",
                "stable() { return 1; }",
            ),
            (
                "installFieldValueStable(this.stable)",
                "stable() { return 3; }",
            ),
            ("installStaticField(this.handle)", "handle() { return 4; }"),
            (
                "installStaticFieldStable(this.stable)",
                "stable() { return 4; }",
            ),
            ("installShadowStable(this.stable)", "stable() { return 5; }"),
            (
                "installShadowBracketStable(this.stable)",
                "stable() { return 6; }",
            ),
            (
                "installEscapedWriteStable(this.stable)",
                "stable() { return 7; }",
            ),
            (
                "installEscapedMethodStable(this.stable)",
                "stable() { return 8; }",
            ),
            (
                "installDeclaredStable(this.stable)",
                "stable() { return 9; }",
            ),
            (
                "installDefiniteStable(this.stable)",
                "stable() { return 10; }",
            ),
        ],
    );
}

#[test]
fn javascript_jsx_attribute_values_are_value_uses() {
    check(
        r#"function handleClick() {}
class Panel {
    handle() {}
    stable() {}
    swap(fn) { this.handle = fn; }
    render() {
        return <div><Button onClick={handleClick} cb={this.stable} other={this.handle} label="x" /></div>;
    }
}
function shadowed(handleClick) { return <Button onPress={handleClick} />; }
function unknown() { return <Button onHover={missing} />; }
"#,
        LanguageId::JavaScript,
        &[
            "other={this.handle}",
            "onPress={handleClick}",
            "onHover={missing}",
        ],
        &[
            ("onClick={handleClick}", "function handleClick() {}"),
            ("cb={this.stable}", "stable() {}"),
        ],
    );
}

#[test]
fn later_method_names_and_outer_this_never_give_an_exact_method_callback() {
    check(
        r#"const key = "x";
function register() { return () => {}; }
class StringName {
    handle() { return 1; }
    stable() { return 1; }
    "handle"() { return 0; }
    bind() {
        installStringName(this.handle);
        installStringNameStable(this.stable);
    }
}
class EscapedName {
    handle() { return 2; }
    stable() { return 2; }
    h\u0061ndle() { return 0; }
    bind() {
        installEscapedName(this.handle);
        installEscapedNameStable(this.stable);
    }
}
class ComputedLiteral {
    handle() { return 3; }
    stable() { return 3; }
    ["handle"]() { return 0; }
    bind() {
        installComputedLiteral(this.handle);
        installComputedLiteralStable(this.stable);
    }
}
class ComputedName {
    handle() { return 4; }
    stable() { return 4; }
    [key]() { return 0; }
    bind() {
        installComputedName(this.handle);
        installComputedNameStable(this.stable);
    }
}
class Decorated {
    handle() { return 5; }
    stable() { return 5; }
    @register(this.handle) other() { return 0; }
    bind() {
        installDecoratedStable(this.stable);
    }
}
"#,
        LanguageId::JavaScript,
        &[
            "installStringName(this.handle)",
            "installEscapedName(this.handle)",
            "installComputedLiteral(this.handle)",
            "installComputedName(this.handle)",
            "installComputedNameStable(this.stable)",
            "register(this.handle)",
        ],
        &[
            (
                "installStringNameStable(this.stable)",
                "stable() { return 1; }",
            ),
            (
                "installEscapedNameStable(this.stable)",
                "stable() { return 2; }",
            ),
            (
                "installComputedLiteralStable(this.stable)",
                "stable() { return 3; }",
            ),
            (
                "installDecoratedStable(this.stable)",
                "stable() { return 5; }",
            ),
        ],
    );
    check(
        r#"const key = "x";
function register() { return () => {}; }
class StringName {
    handle() { return 1; }
    stable() { return 1; }
    "handle"() { return 0; }
    bind() {
        installStringName(this.handle);
        installStringNameStable(this.stable);
    }
}
class EscapedName {
    handle() { return 2; }
    stable() { return 2; }
    h\u0061ndle() { return 0; }
    bind() {
        installEscapedName(this.handle);
        installEscapedNameStable(this.stable);
    }
}
class ComputedLiteral {
    handle() { return 3; }
    stable() { return 3; }
    ["handle"]() { return 0; }
    bind() {
        installComputedLiteral(this.handle);
        installComputedLiteralStable(this.stable);
    }
}
class ComputedName {
    handle() { return 4; }
    stable() { return 4; }
    [key]() { return 0; }
    bind() {
        installComputedName(this.handle);
        installComputedNameStable(this.stable);
    }
}
class Decorated {
    handle() { return 5; }
    stable() { return 5; }
    @register(this.handle) other() { return 0; }
    bind() {
        installDecoratedStable(this.stable);
    }
}
"#,
        LanguageId::TypeScript,
        &[
            "installStringName(this.handle)",
            "installEscapedName(this.handle)",
            "installComputedLiteral(this.handle)",
            "installComputedName(this.handle)",
            "installComputedNameStable(this.stable)",
            "register(this.handle)",
        ],
        &[
            (
                "installStringNameStable(this.stable)",
                "stable() { return 1; }",
            ),
            (
                "installEscapedNameStable(this.stable)",
                "stable() { return 2; }",
            ),
            (
                "installComputedLiteralStable(this.stable)",
                "stable() { return 3; }",
            ),
            (
                "installDecoratedStable(this.stable)",
                "stable() { return 5; }",
            ),
        ],
    );
}
