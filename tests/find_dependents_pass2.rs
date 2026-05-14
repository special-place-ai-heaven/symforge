use symforge::domain::{
    FileClassification, LanguageId, ReferenceKind, ReferenceRecord, SymbolKind, SymbolRecord,
};
use symforge::live_index::{IndexedFile, LiveIndex, ParseStatus, SharedIndex};

fn symbol(name: &str, kind: SymbolKind) -> SymbolRecord {
    SymbolRecord {
        name: name.to_string(),
        kind,
        depth: 0,
        sort_order: 0,
        byte_range: (0, name.len() as u32),
        line_range: (0, 0),
        doc_byte_range: None,
        item_byte_range: None,
    }
}

fn reference(
    name: &str,
    qualified_name: Option<&str>,
    kind: ReferenceKind,
    line: u32,
) -> ReferenceRecord {
    ReferenceRecord {
        name: name.to_string(),
        qualified_name: qualified_name.map(str::to_string),
        kind,
        byte_range: (line * 10, line * 10 + name.len() as u32),
        line_range: (line, line),
        enclosing_symbol_index: Some(0),
    }
}

fn indexed_file(
    relative_path: &str,
    language: LanguageId,
    content: &str,
    symbols: Vec<SymbolRecord>,
    references: Vec<ReferenceRecord>,
) -> IndexedFile {
    IndexedFile {
        relative_path: relative_path.to_string(),
        language,
        classification: FileClassification::for_code_path(relative_path),
        content: content.as_bytes().to_vec(),
        symbols,
        parse_status: ParseStatus::Parsed,
        parse_diagnostic: None,
        byte_len: content.len() as u64,
        content_hash: String::new(),
        references,
        alias_map: Default::default(),
        mtime_secs: 0,
    }
}

fn build_index(files: Vec<(&str, IndexedFile)>) -> SharedIndex {
    let shared = LiveIndex::empty();
    {
        let mut index = shared.write();
        for (path, file) in files {
            index.update_file(path.to_string(), file);
        }
    }
    shared
}

fn dependent_refs_for(
    index: &LiveIndex,
    target_path: &str,
) -> Vec<(String, ReferenceKind, String, Option<String>)> {
    index
        .find_dependents_for_file(target_path)
        .into_iter()
        .map(|(path, reference)| {
            (
                path.to_string(),
                reference.kind,
                reference.name.clone(),
                reference.qualified_name.clone(),
            )
        })
        .collect()
}

fn collision_call_count(
    refs: &[(String, ReferenceKind, String, Option<String>)],
    path_marker: &str,
) -> usize {
    refs.iter()
        .filter(|(path, kind, _, _)| path.contains(path_marker) && *kind == ReferenceKind::Call)
        .count()
}

mod find_dependents {
    use super::*;

    #[test]
    fn constructor_name_collision_no_false_positive() {
        let target = indexed_file(
            "src/target.rs",
            LanguageId::Rust,
            r#"
pub struct TypeA;

impl TypeA {
    pub fn new() -> Self {
        Self
    }
}
"#,
            vec![
                symbol("TypeA", SymbolKind::Struct),
                symbol("new", SymbolKind::Function),
            ],
            vec![],
        );
        let candidate = indexed_file(
            "src/candidate.rs",
            LanguageId::Rust,
            r#"
use crate::target::unrelated;

pub fn build_values() {
    let _values: Vec<i32> = Vec::new();
    let _name = String::new();
    let _other = unrelated::Other::new();
}
"#,
            vec![symbol("build_values", SymbolKind::Function)],
            vec![
                reference(
                    "unrelated",
                    Some("crate::target::unrelated"),
                    ReferenceKind::Import,
                    1,
                ),
                reference("new", Some("Vec::new"), ReferenceKind::Call, 4),
                reference("new", Some("String::new"), ReferenceKind::Call, 5),
                reference("new", Some("unrelated::Other::new"), ReferenceKind::Call, 6),
            ],
        );
        let shared = build_index(vec![("src/target.rs", target), ("src/candidate.rs", candidate)]);
        let index = shared.read();

        let refs = dependent_refs_for(&index, "src/target.rs");

        assert_eq!(
            collision_call_count(&refs, "src/candidate.rs"),
            0,
            "constructor-name collisions should not be promoted as dependent refs; got {refs:?}"
        );
    }

    #[test]
    fn real_qualified_call_dependent_still_reported() {
        let target = indexed_file(
            "src/target.rs",
            LanguageId::Rust,
            "pub fn new() {}\n",
            vec![symbol("new", SymbolKind::Function)],
            vec![],
        );
        let caller = indexed_file(
            "src/caller.rs",
            LanguageId::Rust,
            r#"
pub fn make_type() {
    let _value = target::new();
}
"#,
            vec![symbol("make_type", SymbolKind::Function)],
            vec![reference("new", Some("target::new"), ReferenceKind::Call, 2)],
        );
        let shared = build_index(vec![("src/target.rs", target), ("src/caller.rs", caller)]);
        let index = shared.read();

        let refs = dependent_refs_for(&index, "src/target.rs");

        assert!(
            refs.iter()
                .any(|(path, kind, name, _)| path == "src/caller.rs"
                    && *kind == ReferenceKind::Call
                    && name == "new"),
            "qualified target::new call should remain a dependent ref; got {refs:?}"
        );
    }

    #[test]
    fn cross_language_method_name_collision() {
        let csharp_target = indexed_file(
            "csharp/TypeA.cs",
            LanguageId::CSharp,
            r#"
namespace Shared.Models
{
    public class TypeA
    {
        public override bool Equals(object? obj) => obj is TypeA;
    }
}
"#,
            vec![
                symbol("TypeA", SymbolKind::Class),
                symbol("Equals", SymbolKind::Method),
            ],
            vec![],
        );
        let csharp_collision = indexed_file(
            "csharp/Consumer.cs",
            LanguageId::CSharp,
            r#"
namespace Shared.Models
{
    public class Consumer
    {
        public bool Check(object obj, object other)
        {
            return obj.Equals(other) || string.Equals("a", "b");
        }
    }
}
"#,
            vec![symbol("Consumer", SymbolKind::Class)],
            vec![
                reference("Equals", Some("obj.Equals"), ReferenceKind::Call, 7),
                reference("Equals", Some("string.Equals"), ReferenceKind::Call, 7),
            ],
        );
        let python_target = indexed_file(
            "module.py",
            LanguageId::Python,
            "def foo():\n    return 'target'\n",
            vec![symbol("foo", SymbolKind::Function)],
            vec![],
        );
        let python_consumer = indexed_file(
            "python_consumer.py",
            LanguageId::Python,
            r#"
from module import foo
from baz import bar2

def run():
    foo()
    bar2()
"#,
            vec![symbol("run", SymbolKind::Function)],
            vec![
                reference("foo", Some("module.foo"), ReferenceKind::Import, 1),
                reference("bar2", Some("baz.bar2"), ReferenceKind::Import, 2),
                reference("foo", None, ReferenceKind::Call, 5),
                reference("bar2", None, ReferenceKind::Call, 6),
            ],
        );
        let python_noise = indexed_file(
            "python_noise.py",
            LanguageId::Python,
            r#"
from baz import bar2

def run():
    bar2()
"#,
            vec![symbol("run", SymbolKind::Function)],
            vec![
                reference("bar2", Some("baz.bar2"), ReferenceKind::Import, 1),
                reference("bar2", None, ReferenceKind::Call, 4),
            ],
        );
        let shared = build_index(vec![
            ("csharp/TypeA.cs", csharp_target),
            ("csharp/Consumer.cs", csharp_collision),
            ("module.py", python_target),
            ("python_consumer.py", python_consumer),
            ("python_noise.py", python_noise),
        ]);
        let index = shared.read();

        let csharp_refs = dependent_refs_for(&index, "csharp/TypeA.cs");
        assert_eq!(
            collision_call_count(&csharp_refs, "csharp/Consumer.cs"),
            0,
            "unqualified C# Equals calls should not create TypeA.cs dependent refs; got {csharp_refs:?}"
        );

        let python_refs = dependent_refs_for(&index, "module.py");
        assert!(
            python_refs
                .iter()
                .any(|(path, _, _, _)| path == "python_consumer.py"),
            "Python explicit import of module.foo should remain a dependent edge; got {python_refs:?}"
        );
        assert!(
            !python_refs
                .iter()
                .any(|(path, _, _, _)| path == "python_noise.py"),
            "unrelated Python imports/calls must not create module.py dependent refs; got {python_refs:?}"
        );
    }

    #[test]
    fn synthetic_large_method_collision_false_positive_count_under_limit() {
        const NOISY_FILE_COUNT: usize = 1_000;

        let target = indexed_file(
            "src/target.rs",
            LanguageId::Rust,
            r#"
pub struct TypeA;

impl TypeA {
    pub fn new() -> Self {
        Self
    }
}
"#,
            vec![
                symbol("TypeA", SymbolKind::Struct),
                symbol("new", SymbolKind::Function),
            ],
            vec![],
        );
        let real = indexed_file(
            "src/real.rs",
            LanguageId::Rust,
            "pub fn make_real() { let _value = target::new(); }\n",
            vec![symbol("make_real", SymbolKind::Function)],
            vec![reference("new", Some("target::new"), ReferenceKind::Call, 0)],
        );

        let mut files = Vec::with_capacity(NOISY_FILE_COUNT + 2);
        files.push(("src/target.rs".to_string(), target));
        files.push(("src/real.rs".to_string(), real));
        for i in 0..NOISY_FILE_COUNT {
            let path = format!("src/noisy_{i}.rs");
            let file = indexed_file(
                &path,
                LanguageId::Rust,
                &format!(
                    r#"
use crate::target::TypeA as ImportedTypeA{i};

pub fn build_{i}() {{
    let _values: Vec<i32> = Vec::new();
    let _name = String::new();
    let _other = unrelated::Other::new();
}}
"#
                ),
                vec![symbol(&format!("build_{i}"), SymbolKind::Function)],
                vec![
                    reference(
                        "TypeA",
                        Some("crate::target::TypeA"),
                        ReferenceKind::Import,
                        1,
                    ),
                    reference("new", Some("Vec::new"), ReferenceKind::Call, 4),
                    reference("new", Some("String::new"), ReferenceKind::Call, 5),
                    reference("new", Some("unrelated::Other::new"), ReferenceKind::Call, 6),
                ],
            );
            files.push((path, file));
        }

        let shared = build_index(
            files
                .iter()
                .map(|(path, file)| (path.as_str(), file.clone()))
                .collect(),
        );
        let index = shared.read();

        let refs = dependent_refs_for(&index, "src/target.rs");
        let false_positive_count = collision_call_count(&refs, "src/noisy_");

        assert!(
            false_positive_count < 5,
            "synthetic method-name-collision false positives: {false_positive_count}; refs: {refs:?}"
        );
    }
}
