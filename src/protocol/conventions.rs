//! Project conventions detection: analyzes the indexed codebase to infer
//! coding patterns, naming conventions, error handling style, test organization,
//! and file structure. Useful for LLMs writing code that fits the project.

use crate::live_index::store::LiveIndex;

/// Detected project conventions from static analysis of the index.
pub struct ProjectConventions {
    /// Dominant code language of the project (e.g. "Rust", "TypeScript"),
    /// with an optional note when a second language has a >25% share.
    pub language: String,
    pub error_handling: String,
    pub naming: String,
    pub test_patterns: String,
    pub common_imports: Vec<String>,
    pub file_organization: String,
    pub complexity: String,
}

/// Map an `IndexedFile.language` to a stable, human-readable code-language
/// bucket for the dominant-language vote, or `None` for config/markup files
/// that must be excluded from the vote.
///
/// JavaScript and TypeScript fold into a single `"TypeScript"` bucket so a
/// mixed `.js`/`.ts` codebase does not split its own vote. Config and markup
/// languages (Json/Toml/Yaml/Markdown/Env/Html/Css/Scss) are excluded by
/// `LanguageId` rather than by `FileClass` — every code path is classified
/// `FileClass::Code`, so a `FileClass`-based filter would let `.json` fixtures
/// win the vote.
fn code_language_bucket(language: &crate::domain::index::LanguageId) -> Option<&'static str> {
    use crate::domain::index::LanguageId;
    match language {
        LanguageId::Rust => Some("Rust"),
        LanguageId::Python => Some("Python"),
        // Fold JS + TS into one bucket so the vote is not split.
        LanguageId::JavaScript | LanguageId::TypeScript => Some("TypeScript"),
        LanguageId::Go => Some("Go"),
        LanguageId::Java => Some("Java"),
        LanguageId::C => Some("C"),
        LanguageId::Cpp => Some("C++"),
        LanguageId::CSharp => Some("C#"),
        LanguageId::Ruby => Some("Ruby"),
        LanguageId::Php => Some("PHP"),
        LanguageId::Swift => Some("Swift"),
        LanguageId::Kotlin => Some("Kotlin"),
        LanguageId::Dart => Some("Dart"),
        LanguageId::Perl => Some("Perl"),
        LanguageId::Elixir => Some("Elixir"),
        // Config / markup languages never vote on the dominant CODE language.
        LanguageId::Json
        | LanguageId::Toml
        | LanguageId::Yaml
        | LanguageId::Markdown
        | LanguageId::Env
        | LanguageId::Html
        | LanguageId::Css
        | LanguageId::Scss => None,
    }
}

/// Heuristic camelCase check for TS/JS function/method names: starts with a
/// lowercase ASCII letter and contains no underscores. Treats single-word
/// lowercase identifiers (e.g. `handle`) as camelCase, matching the JS/TS
/// convention. Names starting with `_` (private) or `$` are not counted.
fn is_camel_case(name: &str) -> bool {
    match name.chars().next() {
        Some(c) => c.is_ascii_lowercase() && !name.contains('_'),
        None => false,
    }
}

/// Analyze the index to detect project conventions.
///
/// The analysis is language-aware: it first votes on the project's dominant
/// code language by tallying `IndexedFile.language` over non-config code files,
/// then reports error-handling, naming, and test conventions framed for that
/// language. Without this, a TypeScript/NestJS project would be described in
/// Rust terms ("Result-based", "% snake_case", inline test modules) because the
/// substring heuristics fire on any `Result<T>` type and Rust-only symbol scans.
pub fn detect_conventions(index: &LiveIndex) -> ProjectConventions {
    use crate::domain::index::LanguageId;

    // ── Pass 1: dominant-language vote ───────────────────────────────────────
    // Tally over CODE files only, excluding config/markup by `LanguageId`
    // (NOT by `FileClass`, which is always `Code` — a `.json` fixture would
    // otherwise win the vote). JS and TS are folded into one bucket so a mixed
    // `.js`/`.ts` frontend does not split its own vote.
    let mut lang_votes: std::collections::HashMap<&'static str, u32> =
        std::collections::HashMap::new();
    for (_path, file) in index.all_files() {
        if let Some(bucket) = code_language_bucket(&file.language) {
            *lang_votes.entry(bucket).or_insert(0) += 1;
        }
    }
    let total_code_votes: u32 = lang_votes.values().copied().sum();
    let mut vote_vec: Vec<(&'static str, u32)> = lang_votes.into_iter().collect();
    // Deterministic ordering: by count desc, then name asc for tie-breaking.
    vote_vec.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    let primary_lang: &'static str = vote_vec.first().map(|(name, _)| *name).unwrap_or("Unknown");
    // Open decision (recorded): a mixed repo reports a SINGLE primary language
    // plus a one-line note when a SECOND language holds a >25% share.
    let secondary_note: Option<String> = vote_vec.get(1).and_then(|(name, count)| {
        if total_code_votes > 0 && (*count as u64) * 100 > (total_code_votes as u64) * 25 {
            let pct = (*count * 100).checked_div(total_code_votes).unwrap_or(0);
            Some(format!("{name} also {pct}%"))
        } else {
            None
        }
    });

    // ── Pass 2: per-file scan (each counter gated by THAT file's language) ────
    // Rust counters.
    let mut error_result_count = 0u32;
    let mut error_anyhow_count = 0u32;
    let mut error_thiserror_count = 0u32;
    let mut unwrap_count = 0u32;
    let mut expect_count = 0u32;
    let mut snake_case_fns = 0u32;
    let mut camel_case_types = 0u32;
    let mut rust_total_fns = 0u32;
    let mut rust_total_types = 0u32;
    let mut inline_test_mod_count = 0u32;
    let mut test_fn_count = 0u32;

    // TS/JS counters.
    let mut try_catch_count = 0u32;
    let mut throw_new_count = 0u32;
    let mut http_exception_count = 0u32;
    let mut catch_error_count = 0u32;
    let mut camel_case_fns = 0u32;
    let mut pascal_case_types = 0u32;
    let mut ts_total_fns = 0u32;
    let mut ts_total_types = 0u32;
    let mut describe_block_count = 0u32;
    let mut decorator_files = 0u32;
    let mut dto_validator_files = 0u32;
    let mut signal_files = 0u32;

    // Language-agnostic counters.
    let mut test_file_count = 0u32;

    let mut import_counts: std::collections::HashMap<String, u32> =
        std::collections::HashMap::new();

    let mut total_files = 0u32;
    let mut total_symbols = 0u32;
    let mut max_symbols_per_file = 0u32;
    let mut total_file_bytes = 0u64;
    let mut max_file_bytes = 0u64;
    let mut code_file_count = 0u32;

    for (_path, file) in index.all_files() {
        total_files += 1;
        total_symbols += file.symbols.len() as u32;
        total_file_bytes += file.byte_len;

        if file.symbols.len() as u32 > max_symbols_per_file {
            max_symbols_per_file = file.symbols.len() as u32;
        }
        if file.byte_len > max_file_bytes {
            max_file_bytes = file.byte_len;
        }

        let is_code = matches!(
            file.classification.class,
            crate::domain::index::FileClass::Code
        );
        if is_code {
            code_file_count += 1;
        }

        // Test detection (language-agnostic: `is_test` already covers Rust
        // `test_`/`_test` and TS/JS `*.spec.ts`/`*.test.ts` naming).
        if file.classification.is_test {
            test_file_count += 1;
        }

        let content_str = std::str::from_utf8(&file.content).unwrap_or("");

        let is_rust = file.language == LanguageId::Rust;
        let is_ts_js = matches!(
            file.language,
            LanguageId::TypeScript | LanguageId::JavaScript
        );

        // Error-handling patterns — gated by the FILE's language, not just the
        // summary branch, so non-Rust files never pollute the `Result`/`anyhow`
        // counts in a Rust-majority mixed repo (and vice versa).
        if is_rust {
            if content_str.contains("Result<") || content_str.contains("-> Result") {
                error_result_count += 1;
            }
            if content_str.contains("anyhow") {
                error_anyhow_count += 1;
            }
            if content_str.contains("thiserror") {
                error_thiserror_count += 1;
            }
            unwrap_count += content_str.matches(".unwrap()").count() as u32;
            expect_count += content_str.matches(".expect(").count() as u32;

            // Inline test modules + `test_`-prefixed fns are Rust-only signals.
            for sym in &file.symbols {
                if sym.name == "tests"
                    && matches!(sym.kind, crate::domain::index::SymbolKind::Module)
                {
                    inline_test_mod_count += 1;
                }
                if sym.name.starts_with("test_")
                    && matches!(sym.kind, crate::domain::index::SymbolKind::Function)
                {
                    test_fn_count += 1;
                }
            }
        } else if is_ts_js {
            if content_str.contains("try {") || content_str.contains("} catch") {
                try_catch_count += 1;
            }
            if content_str.contains("throw new") {
                throw_new_count += 1;
            }
            if content_str.contains("HttpException") {
                http_exception_count += 1;
            }
            if content_str.contains("catchError") {
                catch_error_count += 1;
            }

            // Test framework calls (`describe(`/`it(`/`test(`) — `SymbolRecord`
            // does not store these, so scan the source.
            if content_str.contains("describe(")
                || content_str.contains("it(")
                || content_str.contains("test(")
            {
                describe_block_count += 1;
            }

            // Decorators / DTO validators / signals — `SymbolRecord` does NOT
            // store decorators, so detect them via content scan.
            if content_str.contains("@Controller")
                || content_str.contains("@Injectable")
                || content_str.contains("@Module")
                || content_str.contains("@Component")
            {
                decorator_files += 1;
            }
            if content_str.contains("@IsString")
                || content_str.contains("@IsNumber")
                || content_str.contains("@IsNotEmpty")
            {
                dto_validator_files += 1;
            }
            if content_str.contains("signal(") || content_str.contains("inject(") {
                signal_files += 1;
            }
        }

        // Naming conventions — gated by the file's language so the Rust
        // snake_case tally and the TS/JS camelCase tally never cross-pollute.
        for sym in &file.symbols {
            match sym.kind {
                crate::domain::index::SymbolKind::Function
                | crate::domain::index::SymbolKind::Method => {
                    if is_rust {
                        rust_total_fns += 1;
                        if sym.name.contains('_') && sym.name == sym.name.to_ascii_lowercase() {
                            snake_case_fns += 1;
                        }
                    } else if is_ts_js {
                        ts_total_fns += 1;
                        if is_camel_case(&sym.name) {
                            camel_case_fns += 1;
                        }
                    }
                }
                crate::domain::index::SymbolKind::Struct
                | crate::domain::index::SymbolKind::Class
                | crate::domain::index::SymbolKind::Enum
                | crate::domain::index::SymbolKind::Trait
                | crate::domain::index::SymbolKind::Interface
                | crate::domain::index::SymbolKind::Type => {
                    if is_rust {
                        rust_total_types += 1;
                        if sym.name.chars().next().is_some_and(|c| c.is_uppercase()) {
                            camel_case_types += 1;
                        }
                    } else if is_ts_js {
                        ts_total_types += 1;
                        if sym.name.chars().next().is_some_and(|c| c.is_uppercase()) {
                            pascal_case_types += 1;
                        }
                    }
                }
                _ => {}
            }
        }

        // Import tracking (language-agnostic).
        for reference in &file.references {
            if matches!(reference.kind, crate::domain::index::ReferenceKind::Import) {
                let import_name = reference
                    .qualified_name
                    .as_deref()
                    .unwrap_or(&reference.name);
                // Extract the crate/module root
                let root = import_name.split("::").next().unwrap_or(import_name);
                if !root.is_empty() && root.len() > 1 {
                    *import_counts.entry(root.to_string()).or_insert(0) += 1;
                }
            }
        }
    }

    // ── Language header ──────────────────────────────────────────────────────
    let language = match &secondary_note {
        Some(note) => format!("{primary_lang} (primary; {note})"),
        None => primary_lang.to_string(),
    };

    // ── Error handling summary (language-branched) ───────────────────────────
    let error_handling = match primary_lang {
        "Rust" => {
            if error_anyhow_count > 2 && error_thiserror_count > 2 {
                format!(
                    "Mixed: anyhow ({error_anyhow_count} files) + thiserror ({error_thiserror_count} files), Result<> in {error_result_count} files, {unwrap_count} unwrap()s, {expect_count} expect()s"
                )
            } else if error_anyhow_count > 2 {
                format!(
                    "anyhow-based: {error_anyhow_count} files use anyhow, Result<> in {error_result_count} files, {unwrap_count} unwrap()s, {expect_count} expect()s"
                )
            } else if error_thiserror_count > 2 {
                format!(
                    "thiserror-based: {error_thiserror_count} files use thiserror, Result<> in {error_result_count} files, {unwrap_count} unwrap()s, {expect_count} expect()s"
                )
            } else if error_result_count > 0 {
                format!(
                    "Result-based: {error_result_count} files return Result, {unwrap_count} unwrap()s, {expect_count} expect()s"
                )
            } else {
                format!(
                    "Minimal error handling detected. {unwrap_count} unwrap()s, {expect_count} expect()s"
                )
            }
        }
        "TypeScript" | "JavaScript" => {
            if try_catch_count == 0
                && throw_new_count == 0
                && http_exception_count == 0
                && catch_error_count == 0
            {
                "Minimal explicit error handling detected (no try/catch or throw found).".to_string()
            } else {
                let mut parts: Vec<String> = Vec::new();
                if try_catch_count > 0 {
                    parts.push(format!("try/catch in {try_catch_count} files"));
                }
                if throw_new_count > 0 {
                    parts.push(format!("`throw new` in {throw_new_count} files"));
                }
                if http_exception_count > 0 {
                    parts.push(format!(
                        "NestJS HttpException in {http_exception_count} files"
                    ));
                }
                if catch_error_count > 0 {
                    parts.push(format!("RxJS catchError in {catch_error_count} files"));
                }
                format!("Exception-based: {}", parts.join(", "))
            }
        }
        _ => "Error handling: language-specific heuristics unavailable for this project's dominant language.".to_string(),
    };

    // ── Naming summary (language-branched) ───────────────────────────────────
    let naming = match primary_lang {
        "Rust" => {
            let fn_pct = (snake_case_fns * 100)
                .checked_div(rust_total_fns)
                .unwrap_or(0);
            let type_pct = (camel_case_types * 100)
                .checked_div(rust_total_types)
                .unwrap_or(0);
            format!(
                "Functions: {fn_pct}% snake_case ({snake_case_fns}/{rust_total_fns}). Types: {type_pct}% CamelCase ({camel_case_types}/{rust_total_types})."
            )
        }
        "TypeScript" | "JavaScript" => {
            let fn_pct = (camel_case_fns * 100)
                .checked_div(ts_total_fns)
                .unwrap_or(0);
            let type_pct = (pascal_case_types * 100)
                .checked_div(ts_total_types)
                .unwrap_or(0);
            format!(
                "Functions: {fn_pct}% camelCase ({camel_case_fns}/{ts_total_fns}). Types: {type_pct}% PascalCase ({pascal_case_types}/{ts_total_types})."
            )
        }
        _ => {
            "Naming: language-specific heuristics unavailable for this project's dominant language."
                .to_string()
        }
    };

    // ── Test patterns (language-branched; `test_file_count` is shared) ───────
    let test_patterns = match primary_lang {
        "Rust" => format!(
            "{test_file_count} test files, {inline_test_mod_count} inline test modules, {test_fn_count} test functions"
        ),
        "TypeScript" | "JavaScript" => {
            let mut extras: Vec<String> = Vec::new();
            extras.push(format!(
                "{describe_block_count} files with describe/it/test"
            ));
            if decorator_files > 0 {
                extras.push(format!("{decorator_files} files with decorators"));
            }
            if dto_validator_files > 0 {
                extras.push(format!(
                    "{dto_validator_files} files with class-validator DTOs"
                ));
            }
            if signal_files > 0 {
                extras.push(format!("{signal_files} files with signals/inject"));
            }
            let framework = if describe_block_count > 0 {
                " (Jest/Mocha-style)"
            } else {
                ""
            };
            format!(
                "{test_file_count} test files{framework}, {extras}",
                extras = extras.join(", ")
            )
        }
        _ => format!("{test_file_count} test files"),
    };

    // Common imports (top 10)
    let mut import_vec: Vec<(String, u32)> = import_counts.into_iter().collect();
    import_vec.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    let common_imports: Vec<String> = import_vec
        .iter()
        .take(10)
        .map(|(name, count)| format!("{name} ({count} uses)"))
        .collect();

    // File organization (language-agnostic)
    let avg_symbols = total_symbols.checked_div(code_file_count).unwrap_or(0);
    let avg_size = if code_file_count > 0 {
        total_file_bytes / code_file_count as u64
    } else {
        0
    };
    let file_organization = format!(
        "{total_files} files ({code_file_count} code), avg {avg_symbols} symbols/file, avg {avg_kb}KB/file, largest {max_kb}KB ({max_symbols_per_file} symbols)",
        avg_kb = avg_size / 1024,
        max_kb = max_file_bytes / 1024,
    );

    // Complexity (language-agnostic)
    let complexity = if max_symbols_per_file > 200 {
        format!("High: largest file has {max_symbols_per_file} symbols")
    } else if max_symbols_per_file > 100 {
        format!("Medium: largest file has {max_symbols_per_file} symbols")
    } else {
        format!("Low: largest file has {max_symbols_per_file} symbols")
    };

    ProjectConventions {
        language,
        error_handling,
        naming,
        test_patterns,
        common_imports,
        file_organization,
        complexity,
    }
}

/// Extract the top N import root names from the index (cheap — single pass, no formatting).
/// Returns lowercase crate/module roots like `["serde", "tokio", "anyhow"]`.
///
/// Sources (unioned):
/// 1. `use`/`import` references extracted by tree-sitter (covers explicit imports).
/// 2. Manifest dependencies from `Cargo.toml` / `package.json` (covers crates used
///    only via derive macros, path-qualified syntax, or re-exports that tree-sitter
///    does not classify as imports — e.g. `thiserror`).
pub fn extract_top_import_roots(index: &LiveIndex, limit: usize) -> Vec<String> {
    let mut counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();

    // Source 1: explicit import references from the tree-sitter index.
    for (_path, file) in index.all_files() {
        for reference in &file.references {
            if matches!(reference.kind, crate::domain::index::ReferenceKind::Import) {
                let import_name = reference
                    .qualified_name
                    .as_deref()
                    .unwrap_or(&reference.name);
                let root = import_name.split("::").next().unwrap_or(import_name);
                if root.len() > 1 {
                    *counts.entry(root.to_ascii_lowercase()).or_insert(0) += 1;
                }
            }
        }
    }

    // Source 2: manifest dependency names (Cargo.toml / package.json).
    // These ensure crates used only via derive macros or path-qualified syntax
    // (e.g. `#[derive(thiserror::Error)]`) still appear in the import list.
    for (path, file) in index.all_files() {
        let is_cargo = path.ends_with("Cargo.toml");
        let is_package_json = path.ends_with("package.json");
        if !is_cargo && !is_package_json {
            continue;
        }
        for sym in &file.symbols {
            // Cargo.toml: "dependencies.thiserror", "dev-dependencies.once_cell"
            // package.json: "dependencies.express", "devDependencies.lodash"
            let dep_name = if is_cargo {
                sym.name
                    .strip_prefix("dependencies.")
                    .or_else(|| sym.name.strip_prefix("dev-dependencies."))
            } else {
                sym.name
                    .strip_prefix("dependencies.")
                    .or_else(|| sym.name.strip_prefix("devDependencies."))
            };
            if let Some(raw) = dep_name {
                // Skip nested sub-keys like "dependencies.serde.version".
                if raw.contains('.') {
                    continue;
                }
                // Normalize: Cargo crate hyphens → underscores (e.g. tree-sitter → tree_sitter).
                let normalized = if is_cargo {
                    raw.replace('-', "_").to_ascii_lowercase()
                } else {
                    raw.to_ascii_lowercase()
                };
                if normalized.len() > 1 {
                    counts.entry(normalized).or_insert(1);
                }
            }
        }
    }

    let mut vec: Vec<(String, u32)> = counts.into_iter().collect();
    vec.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    vec.into_iter().take(limit).map(|(name, _)| name).collect()
}

/// Format conventions for display.
pub fn format_conventions(conv: &ProjectConventions) -> String {
    let mut lines = vec![
        "── Project Conventions ──".to_string(),
        String::new(),
        format!("Language: {}", conv.language),
        format!("Error handling: {}", conv.error_handling),
        format!("Naming: {}", conv.naming),
        format!("Tests: {}", conv.test_patterns),
        format!("File organization: {}", conv.file_organization),
        format!("Complexity: {}", conv.complexity),
    ];

    if !conv.common_imports.is_empty() {
        lines.push(String::new());
        lines.push("Common imports:".to_string());
        for import in &conv.common_imports {
            lines.push(format!("  {import}"));
        }
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::index::{
        FileClassification, LanguageId, ReferenceKind, ReferenceRecord, SymbolKind, SymbolRecord,
    };
    use crate::live_index::store::IndexedFile;

    /// Build a `SymbolRecord` with the given name and kind. Byte/line ranges are
    /// placeholders — `detect_conventions` only reads `name` and `kind`.
    fn sym(name: &str, kind: SymbolKind) -> SymbolRecord {
        SymbolRecord {
            name: name.to_string(),
            kind,
            depth: 0,
            sort_order: 0,
            byte_range: (0, 0),
            line_range: (0, 0),
            doc_byte_range: None,
            item_byte_range: None,
        }
    }

    fn import_ref(name: &str) -> ReferenceRecord {
        ReferenceRecord {
            name: name.to_string(),
            qualified_name: None,
            kind: ReferenceKind::Import,
            byte_range: (0, 0),
            line_range: (0, 0),
            enclosing_symbol_index: None,
        }
    }

    /// Build an `IndexedFile` with the language wired EXPLICITLY (per verifier:
    /// not left defaulted) so a test that asserts language-aware behavior cannot
    /// pass for the wrong reason.
    fn make_file(
        path: &str,
        language: LanguageId,
        content: &str,
        symbols: Vec<SymbolRecord>,
        references: Vec<ReferenceRecord>,
    ) -> IndexedFile {
        let bytes = content.as_bytes().to_vec();
        let byte_len = bytes.len() as u64;
        IndexedFile {
            relative_path: path.to_string(),
            language,
            classification: FileClassification::for_code_path(path),
            content: bytes,
            symbols,
            parse_status: crate::live_index::store::ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len,
            content_hash: "hash".to_string(),
            references,
            alias_map: std::collections::HashMap::new(),
            mtime_secs: 0,
        }
    }

    /// Assemble a `LiveIndex` from fixture files via the public shared-index
    /// API and run `detect_conventions`.
    fn conventions_for(files: Vec<(&str, IndexedFile)>) -> ProjectConventions {
        let shared = LiveIndex::empty();
        {
            let mut guard = shared.write();
            for (path, file) in files {
                guard.add_file(path.to_string(), file);
            }
        }
        let guard = shared.read();
        detect_conventions(&guard)
    }

    #[test]
    fn rust_majority_project_reports_result_snake_case_and_test_modules() {
        let lib = make_file(
            "src/lib.rs",
            LanguageId::Rust,
            "use anyhow::Result;\nfn do_thing() -> Result<()> { foo().unwrap(); Ok(()) }\n",
            vec![
                sym("do_thing", SymbolKind::Function),
                sym("MyStruct", SymbolKind::Struct),
            ],
            vec![import_ref("anyhow")],
        );
        let store = make_file(
            "src/store.rs",
            LanguageId::Rust,
            "fn load_store() -> Result<u8> { Ok(0) }\n",
            vec![sym("load_store", SymbolKind::Function)],
            vec![],
        );
        // A Rust test file with an inline `tests` module + a `test_` fn.
        let test_file = make_file(
            "src/store_tests.rs",
            LanguageId::Rust,
            "#[cfg(test)] mod tests { fn test_load() {} }\n",
            vec![
                sym("tests", SymbolKind::Module),
                sym("test_load", SymbolKind::Function),
            ],
            vec![],
        );
        // Config noise must NOT win the vote or pollute the Rust pass.
        let pkg = make_file(
            "config.json",
            LanguageId::Json,
            "{ \"name\": \"x\", \"result\": \"Result<T>\" }\n",
            vec![],
            vec![],
        );

        let conv = conventions_for(vec![
            ("src/lib.rs", lib),
            ("src/store.rs", store),
            ("src/store_tests.rs", test_file),
            ("config.json", pkg),
        ]);

        assert_eq!(conv.language, "Rust", "Rust must win the dominant vote");
        assert!(
            conv.error_handling.contains("Result"),
            "Rust error handling should mention Result, got: {}",
            conv.error_handling
        );
        assert!(
            conv.naming.contains("snake_case"),
            "Rust naming should mention snake_case, got: {}",
            conv.naming
        );
        assert!(
            conv.test_patterns.contains("inline test module")
                && !conv.test_patterns.contains("0 inline test modules"),
            "Rust test patterns should report nonzero inline test modules, got: {}",
            conv.test_patterns
        );
    }

    #[test]
    fn typescript_majority_project_reports_exceptions_camelcase_and_no_result_wording() {
        // NestJS controller with decorators, exceptions, and a `Result<T>` type
        // that must NOT trigger Rust "Result-based" wording.
        let controller = make_file(
            "src/users/users.controller.ts",
            LanguageId::TypeScript,
            "@Controller('users')\nexport class UsersController {\n  getUser() {\n    if (!ok) { throw new HttpException('no', 404); }\n  }\n}\ninterface Result<T> { ok: boolean; value: T; }\n",
            vec![
                sym("UsersController", SymbolKind::Class),
                sym("getUser", SymbolKind::Method),
                sym("Result", SymbolKind::Interface),
            ],
            vec![import_ref("@nestjs/common")],
        );
        let service = make_file(
            "src/users/users.service.ts",
            LanguageId::TypeScript,
            "@Injectable()\nexport class UsersService {\n  findAll() {\n    try { return this.repo.find(); } catch (e) { throw new Error('x'); }\n  }\n}\n",
            vec![
                sym("UsersService", SymbolKind::Class),
                sym("findAll", SymbolKind::Method),
            ],
            vec![],
        );
        let dto = make_file(
            "src/users/create-user.dto.ts",
            LanguageId::TypeScript,
            "export class CreateUserDto {\n  @IsString()\n  name: string;\n}\n",
            vec![sym("CreateUserDto", SymbolKind::Class)],
            vec![],
        );
        // A spec test with describe(/it(.
        let spec = make_file(
            "src/users/users.controller.spec.ts",
            LanguageId::TypeScript,
            "describe('UsersController', () => {\n  it('returns a user', () => { expect(1).toBe(1); });\n});\n",
            vec![],
            vec![],
        );
        // Config noise must NOT win the vote.
        let pkg = make_file(
            "package.json",
            LanguageId::Json,
            "{ \"name\": \"app\" }\n",
            vec![],
            vec![],
        );

        let conv = conventions_for(vec![
            ("src/users/users.controller.ts", controller),
            ("src/users/users.service.ts", service),
            ("src/users/create-user.dto.ts", dto),
            ("src/users/users.controller.spec.ts", spec),
            ("package.json", pkg),
        ]);

        assert_eq!(
            conv.language, "TypeScript",
            "TypeScript must win the dominant vote"
        );
        assert!(
            !conv.error_handling.contains("Result-based"),
            "TS error handling must not say Result-based, got: {}",
            conv.error_handling
        );
        assert!(
            conv.error_handling.contains("Exception-based")
                || conv.error_handling.to_lowercase().contains("exception")
                || conv.error_handling.contains("throw new"),
            "TS error handling should mention exceptions/throw, got: {}",
            conv.error_handling
        );
        assert!(
            conv.naming.contains("camelCase"),
            "TS naming should mention camelCase, got: {}",
            conv.naming
        );
        assert!(
            !conv.test_patterns.starts_with("0 test files"),
            "TS test count should be nonzero, got: {}",
            conv.test_patterns
        );
        assert!(
            conv.test_patterns.contains("describe/it/test"),
            "TS test patterns should mention describe/it/test scan, got: {}",
            conv.test_patterns
        );
    }

    #[test]
    fn mixed_repo_records_secondary_language_note_over_25pct() {
        // 3 Rust + 2 TS code files -> TS share 40% (>25%) -> primary Rust + note.
        let files = vec![
            (
                "a.rs",
                make_file("a.rs", LanguageId::Rust, "fn a() {}\n", vec![], vec![]),
            ),
            (
                "b.rs",
                make_file("b.rs", LanguageId::Rust, "fn b() {}\n", vec![], vec![]),
            ),
            (
                "c.rs",
                make_file("c.rs", LanguageId::Rust, "fn c() {}\n", vec![], vec![]),
            ),
            (
                "d.ts",
                make_file(
                    "d.ts",
                    LanguageId::TypeScript,
                    "function d() {}\n",
                    vec![],
                    vec![],
                ),
            ),
            (
                "e.ts",
                make_file(
                    "e.ts",
                    LanguageId::TypeScript,
                    "function e() {}\n",
                    vec![],
                    vec![],
                ),
            ),
        ];
        let conv = conventions_for(files);
        assert!(
            conv.language.starts_with("Rust"),
            "primary language should be Rust, got: {}",
            conv.language
        );
        assert!(
            conv.language.contains("TypeScript") && conv.language.contains('%'),
            "secondary language note (>25%) should mention TypeScript with a percentage, got: {}",
            conv.language
        );
    }

    #[test]
    fn js_and_ts_fold_into_one_bucket() {
        // 2 JS + 2 TS would split 2/2 if not folded; folded they total 4 and win
        // over a single Rust file.
        let files = vec![
            (
                "x.rs",
                make_file("x.rs", LanguageId::Rust, "fn x() {}\n", vec![], vec![]),
            ),
            (
                "a.js",
                make_file(
                    "a.js",
                    LanguageId::JavaScript,
                    "function a() {}\n",
                    vec![],
                    vec![],
                ),
            ),
            (
                "b.js",
                make_file(
                    "b.js",
                    LanguageId::JavaScript,
                    "function b() {}\n",
                    vec![],
                    vec![],
                ),
            ),
            (
                "c.ts",
                make_file(
                    "c.ts",
                    LanguageId::TypeScript,
                    "function c() {}\n",
                    vec![],
                    vec![],
                ),
            ),
            (
                "d.ts",
                make_file(
                    "d.ts",
                    LanguageId::TypeScript,
                    "function d() {}\n",
                    vec![],
                    vec![],
                ),
            ),
        ];
        let conv = conventions_for(files);
        assert_eq!(
            conv.language, "TypeScript",
            "folded JS+TS (4) should beat Rust (1); got: {}",
            conv.language
        );
    }

    #[test]
    fn config_only_index_does_not_panic_and_reports_unknown() {
        let files = vec![
            (
                "a.json",
                make_file("a.json", LanguageId::Json, "{}\n", vec![], vec![]),
            ),
            (
                "b.toml",
                make_file("b.toml", LanguageId::Toml, "x = 1\n", vec![], vec![]),
            ),
        ];
        let conv = conventions_for(files);
        assert_eq!(
            conv.language, "Unknown",
            "config-only index should report Unknown dominant language"
        );
        // Generic branch must not use Rust-specific wording.
        assert!(!conv.error_handling.contains("Result-based"));
        assert!(!conv.naming.contains("snake_case"));
    }

    #[test]
    fn format_conventions_includes_language_header() {
        let conv = ProjectConventions {
            language: "TypeScript".to_string(),
            error_handling: "Exception-based: throw new in 2 files".to_string(),
            naming: "Functions: 100% camelCase (4/4).".to_string(),
            test_patterns: "3 test files".to_string(),
            common_imports: vec![],
            file_organization: "5 files".to_string(),
            complexity: "Low".to_string(),
        };
        let out = format_conventions(&conv);
        assert!(
            out.contains("Language: TypeScript"),
            "format output should include a Language header, got:\n{out}"
        );
    }
}
