//! An auxiliary index of AST evidence, not a list of guessed references.
//!
//! Import and declaration evidence are independent. Joining against the current
//! generation means changing or deleting an exporter invalidates its uses even
//! when none of its importers were edited. Unresolved candidates never enter
//! the regular xref vector, reference IDs, or call graph.

use std::collections::HashMap;
use std::sync::Arc;

use crate::domain::{LanguageId, ReferenceKind, ReferenceRecord, SymbolKind};
use crate::parsing::xref::callable_values::{self, CallableFacts};

use super::query::import_languages_compatible;
use super::store::{IndexedFile, LiveIndex, ParseStatus};

pub(crate) type CallableValueIndex = HashMap<String, Arc<CallableFacts>>;

pub(super) struct ResolvedValue<'a> {
    pub(super) path: &'a str,
    pub(super) reference: &'a ReferenceRecord,
    pub(super) definition_path: &'a str,
    pub(super) definition_range: (u32, u32),
}

pub(super) fn uses_callable_value_bindings(language: LanguageId) -> bool {
    matches!(
        language,
        LanguageId::Python | LanguageId::JavaScript | LanguageId::TypeScript
    )
}

pub(super) fn file_facts(file: &IndexedFile) -> Option<Arc<CallableFacts>> {
    // Match the source-evidence lane's bound; never read a file from disk or
    // try to recover callable bindings from damaged syntax.
    if !uses_callable_value_bindings(file.language)
        || file.content.len() > 2 * 1024 * 1024
        || file.parse_status != ParseStatus::Parsed
    {
        return None;
    }
    let source = std::str::from_utf8(&file.content).ok()?;
    let export_marker = if file.language == LanguageId::Python {
        "def "
    } else {
        "export"
    };
    let useful = source.contains("import")
        || source.contains(export_marker)
        || (file.language == LanguageId::Python
            && file
                .symbols
                .iter()
                .any(|symbol| symbol.kind == SymbolKind::Class))
        || file
            .references
            .iter()
            .any(|reference| reference.kind == ReferenceKind::ValueUse);
    if !useful {
        return None;
    }
    let tree = crate::parsing::parse_source_tree(
        source,
        &file.language,
        LanguageId::is_tsx_path(&file.relative_path),
    )
    .ok()?;
    let root = tree.root_node();
    if root.has_error() {
        return None;
    }
    let mut facts = callable_values::facts(root, source, file.language);
    if facts.uses.is_empty()
        && facts.exports.is_empty()
        && facts.class_exports.is_empty()
        && facts.locals.is_empty()
    {
        return None;
    }
    let lines: Vec<_> = facts
        .uses
        .iter()
        .map(|candidate| candidate.reference.line_range.0)
        .collect();
    let enclosing = crate::domain::index::find_enclosing_symbols(&file.symbols, &lines);
    for (candidate, enclosing) in facts.uses.iter_mut().zip(enclosing) {
        candidate.reference.enclosing_symbol_index = enclosing;
    }
    Some(Arc::new(facts))
}

pub(super) fn build_callable_values(
    files: &HashMap<String, Arc<IndexedFile>>,
    cancel: Option<&std::sync::atomic::AtomicBool>,
) -> CallableValueIndex {
    let entries: Vec<_> = files.iter().collect();
    super::store::ordered_parallel_map(&entries, |(path, file)| {
        if cancel.is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Acquire)) {
            return None;
        }
        Some(((*path).clone(), file_facts(file)?))
    })
    .into_iter()
    .flatten()
    .collect()
}

impl LiveIndex {
    pub(super) fn local_callable_declaration(
        &self,
        path: &str,
        reference: &ReferenceRecord,
    ) -> Option<(u32, u32)> {
        self.callable_values
            .get(path)?
            .locals
            .get(&reference.byte_range)
            .copied()
    }

    pub(super) fn imported_callable_values<'a>(
        &'a self,
        name: &str,
        target_path: Option<&str>,
    ) -> Vec<ResolvedValue<'a>> {
        self.imported_bound_values(name, target_path, false)
    }

    /// Resolve candidates to a unique declaration in the current generation.
    pub(super) fn imported_bound_values<'a>(
        &'a self,
        name: &str,
        target_path: Option<&str>,
        python_class: bool,
    ) -> Vec<ResolvedValue<'a>> {
        self.callable_values
            .iter()
            .flat_map(move |(path, facts)| {
                facts.uses.iter().filter_map(move |candidate| {
                    // A qualified module attribute is evidence for the class query only.
                    if candidate.reference.qualified_name.is_some() && !python_class {
                        return None;
                    }
                    if candidate.exported != "default"
                        && candidate.exported != name
                        && (target_path.is_some() || candidate.reference.name != name)
                    {
                        return None;
                    }
                    let importer = self.get_file(path)?;
                    let (definition_path, definition) =
                        self.callable_import_target(importer, &candidate.module)?;
                    if target_path.is_some_and(|path| path != definition_path) {
                        return None;
                    }
                    let definition_facts = self.callable_values.get(definition_path)?;
                    // Class values require an exact class selector. Do not
                    // widen callback discovery or the raw name-search lane.
                    let exports = if python_class {
                        &definition_facts.class_exports
                    } else {
                        &definition_facts.exports
                    };
                    let (canonical, definition_range) = exports.get(&candidate.exported)?;
                    if !import_languages_compatible(&importer.language, &definition.language)
                        || (canonical != name
                            && (target_path.is_some() || candidate.reference.name != name))
                    {
                        return None;
                    }
                    Some(ResolvedValue {
                        path,
                        reference: &candidate.reference,
                        definition_path,
                        definition_range: *definition_range,
                    })
                })
            })
            .collect()
    }

    fn callable_import_target<'a>(
        &'a self,
        importer: &IndexedFile,
        module: &str,
    ) -> Option<(&'a str, &'a IndexedFile)> {
        let candidates = if importer.language == LanguageId::Python {
            python_paths(&importer.relative_path, module)?
        } else {
            js_paths(&importer.relative_path, module)?
        };
        // Count all competing modules, including ones without binding facts.
        let mut found = candidates
            .iter()
            .filter_map(|path| self.files.get_key_value(path))
            .filter(|(_, file)| import_languages_compatible(&importer.language, &file.language));
        let (path, target) = found.next()?;
        (found.next().is_none()).then_some((path.as_str(), target.as_ref()))
    }
}

fn python_paths(importer: &str, module: &str) -> Option<Vec<String>> {
    let dots = module.bytes().take_while(|&c| c == b'.').count();
    let suffix = module[dots..].replace('.', "/");
    let mut paths = Vec::with_capacity(4);
    if dots == 0 {
        paths.extend([format!("{suffix}.py"), format!("{suffix}/__init__.py")]);
        paths.extend([
            format!("src/{suffix}.py"),
            format!("src/{suffix}/__init__.py"),
        ]);
    } else {
        let mut parts: Vec<_> = importer.split('/').collect();
        // Exclude the importing filename; each extra dot goes up one package.
        let keep = parts.len().checked_sub(dots).filter(|&keep| keep > 0)?;
        parts.truncate(keep);
        if suffix.is_empty() {
            // `from . import cb` names the containing package, not a
            // same-named sibling module such as `pkg.py`.
            return Some(vec![format!("{}/__init__.py", parts.join("/"))]);
        }
        parts.push(&suffix);
        let base = parts.join("/");
        paths.extend([format!("{base}.py"), format!("{base}/__init__.py")]);
    }
    Some(paths)
}

fn js_paths(importer: &str, module: &str) -> Option<Vec<String>> {
    let mut parts: Vec<_> = importer.split('/').collect();
    parts.pop();
    for part in module.split('/') {
        match part {
            "." => {}
            ".." => {
                parts.pop()?;
            }
            "" => return None,
            _ => parts.push(part),
        }
    }
    let base = parts.join("/");
    if std::path::Path::new(&base).extension().is_some() {
        return Some(vec![base]);
    }
    Some(
        ["js", "jsx", "mjs", "cjs", "ts", "tsx", "mts", "cts"]
            .into_iter()
            .flat_map(|ext| [format!("{base}.{ext}"), format!("{base}/index.{ext}")])
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn parallel_callable_values_equal_the_serial_build() {
        let dir = tempfile::tempdir().unwrap();
        let mut sources = Vec::new();
        for index in 0..40 {
            sources.push((
                format!("pkg/mod{index}.py"),
                format!(
                    "from pkg.mod{next} import handler{next}\n\n\ndef handler{index}():\n    pass\n\n\nregister(handler{next})\n",
                    next = (index + 1) % 40
                ),
            ));
            sources.push((
                format!("web/view{index}.ts"),
                format!(
                    "import {{ cb{next} }} from \"./view{next}\";\nexport function cb{index}(): void {{}}\nexport const run{index} = () => install(cb{next});\n",
                    next = (index + 1) % 40
                ),
            ));
            sources.push((
                format!("app/part{index}.js"),
                format!(
                    "import {{ part{next} }} from \"./part{next}\";\nexport function part{index}() {{}}\ninstall(part{next});\n",
                    next = (index + 1) % 40
                ),
            ));
        }
        for (path, source) in &sources {
            let full = dir.path().join(path);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(full, source).unwrap();
        }
        std::fs::write(dir.path().join("pkg/__init__.py"), "").unwrap();
        let shared = super::LiveIndex::load(dir.path()).unwrap();
        let index = shared.read();
        let serial: super::CallableValueIndex = index
            .files
            .iter()
            .filter_map(|(path, file)| Some((path.clone(), super::file_facts(file)?)))
            .collect();
        let parallel = super::build_callable_values(&index.files, None);
        assert_eq!(serial.len(), 120, "every fixture file has facts");
        assert_eq!(parallel, serial);
        assert_eq!(
            index.callable_values, serial,
            "the load path uses the same build"
        );
        let cancelled = std::sync::atomic::AtomicBool::new(true);
        assert!(super::build_callable_values(&index.files, Some(&cancelled)).is_empty());
    }
}
