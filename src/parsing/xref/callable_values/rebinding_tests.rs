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
        let end = start + usage.rfind(')').expect("callback argument ends in )") as u32;
        let leaf = usage[..(end - start) as usize]
            .rsplit(['.', '('])
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
