//! An auxiliary index of AST evidence, not a list of guessed references.
//!
//! Import and declaration evidence are independent. Joining against the current
//! generation means changing or deleting an exporter invalidates its uses even
//! when none of its importers were edited. Unresolved candidates never enter
//! the regular xref vector, reference IDs, or call graph.

use std::collections::HashMap;
use std::sync::Arc;

use crate::domain::{LanguageId, ReferenceRecord};
use crate::parsing::xref::callable_values::{self, CallableFacts};

use super::query::import_languages_compatible;
use super::store::{IndexedFile, LiveIndex, ParseStatus};

pub(crate) type CallableValueIndex = HashMap<String, Arc<CallableFacts>>;

pub(super) struct ResolvedValue<'a> {
    pub(super) path: &'a str,
    pub(super) reference: &'a ReferenceRecord,
    pub(super) definition_path: &'a str,
    pub(super) definition_offset: u32,
}

pub(super) fn file_facts(file: &IndexedFile) -> Option<Arc<CallableFacts>> {
    // Match the source-evidence lane's bound; never read a file from disk or
    // try to recover callable bindings from damaged syntax.
    if file.content.len() > 2 * 1024 * 1024 || file.parse_status != ParseStatus::Parsed {
        return None;
    }
    let source = std::str::from_utf8(&file.content).ok()?;
    let useful = match file.language {
        LanguageId::Python => source.contains("import") || source.contains("def "),
        LanguageId::JavaScript | LanguageId::TypeScript => {
            source.contains("import") || source.contains("export")
        }
        _ => false,
    };
    if !useful {
        return None;
    }
    let tree = crate::parsing::parse_source_tree(
        source,
        &file.language,
        file.relative_path.ends_with(".tsx"),
    )
    .ok()?;
    let root = tree.root_node();
    if root.has_error() {
        return None;
    }
    let mut facts = callable_values::facts(root, source, file.language);
    if facts.uses.is_empty() && facts.exports.is_empty() {
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

impl LiveIndex {
    /// Resolve candidates to a unique declaration in the current generation.
    pub(super) fn imported_callable_values<'a>(
        &'a self,
        name: &str,
        target_path: Option<&str>,
    ) -> Vec<ResolvedValue<'a>> {
        self.callable_values
            .iter()
            .flat_map(move |(path, facts)| {
                facts.uses.iter().filter_map(move |candidate| {
                    if candidate.exported != "default"
                        && candidate.exported != name
                        && (target_path.is_some() || candidate.reference.name != name)
                    {
                        return None;
                    }
                    let importer = self.get_file(path)?;
                    let (definition_path, definition) =
                        self.callable_import_target(importer, &candidate.module)?;
                    if target_path.is_some_and(|target| target != definition_path) {
                        return None;
                    }
                    let (canonical, offset) = self
                        .callable_values
                        .get(definition_path)?
                        .exports
                        .get(&candidate.exported)?;
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
                        definition_offset: *offset,
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
        // Count all competing modules, not just callable exporters. Ambiguous
        // resolution must not fall through to the one that has a function.
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
