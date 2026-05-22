//! Project conventions detection: analyzes the indexed codebase to infer
//! coding patterns, naming conventions, error handling style, test organization,
//! and file structure. Useful for LLMs writing code that fits the project.

use crate::live_index::store::LiveIndex;

/// Detected project conventions from static analysis of the index.
pub struct ProjectConventions {
    pub error_handling: String,
    pub naming: String,
    pub test_patterns: String,
    pub common_imports: Vec<String>,
    pub file_organization: String,
    pub complexity: String,
}

/// Analyze the index to detect project conventions.
pub fn detect_conventions(index: &LiveIndex) -> ProjectConventions {
    let mut error_result_count = 0u32;
    let mut error_anyhow_count = 0u32;
    let mut error_thiserror_count = 0u32;
    let mut unwrap_count = 0u32;
    let mut expect_count = 0u32;

    let mut snake_case_fns = 0u32;
    let mut camel_case_types = 0u32;
    let mut total_fns = 0u32;
    let mut total_types = 0u32;

    let mut test_file_count = 0u32;
    let mut inline_test_mod_count = 0u32;
    let mut test_fn_count = 0u32;

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

        // Test detection
        if file.classification.is_test {
            test_file_count += 1;
        }

        let content_str = std::str::from_utf8(&file.content).unwrap_or("");

        // Error handling patterns (scan content)
        if is_code {
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
        }

        // Inline test modules
        for sym in &file.symbols {
            if sym.name == "tests" && matches!(sym.kind, crate::domain::index::SymbolKind::Module) {
                inline_test_mod_count += 1;
            }
            if sym.name.starts_with("test_")
                && matches!(sym.kind, crate::domain::index::SymbolKind::Function)
            {
                test_fn_count += 1;
            }
        }

        // Naming conventions
        for sym in &file.symbols {
            match sym.kind {
                crate::domain::index::SymbolKind::Function
                | crate::domain::index::SymbolKind::Method => {
                    total_fns += 1;
                    if sym.name.contains('_') && sym.name == sym.name.to_ascii_lowercase() {
                        snake_case_fns += 1;
                    }
                }
                crate::domain::index::SymbolKind::Struct
                | crate::domain::index::SymbolKind::Class
                | crate::domain::index::SymbolKind::Enum
                | crate::domain::index::SymbolKind::Trait
                | crate::domain::index::SymbolKind::Interface
                | crate::domain::index::SymbolKind::Type => {
                    total_types += 1;
                    if sym.name.chars().next().is_some_and(|c| c.is_uppercase()) {
                        camel_case_types += 1;
                    }
                }
                _ => {}
            }
        }

        // Import tracking
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

    // Error handling summary
    let error_handling = if error_anyhow_count > 2 && error_thiserror_count > 2 {
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
    };

    // Naming summary
    let naming = {
        let fn_pct = (snake_case_fns * 100).checked_div(total_fns).unwrap_or(0);
        let type_pct = (camel_case_types * 100)
            .checked_div(total_types)
            .unwrap_or(0);
        format!(
            "Functions: {fn_pct}% snake_case ({snake_case_fns}/{total_fns}). Types: {type_pct}% CamelCase ({camel_case_types}/{total_types})."
        )
    };

    // Test patterns
    let test_patterns = format!(
        "{test_file_count} test files, {inline_test_mod_count} inline test modules, {test_fn_count} test functions"
    );

    // Common imports (top 10)
    let mut import_vec: Vec<(String, u32)> = import_counts.into_iter().collect();
    import_vec.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    let common_imports: Vec<String> = import_vec
        .iter()
        .take(10)
        .map(|(name, count)| format!("{name} ({count} uses)"))
        .collect();

    // File organization
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

    // Complexity
    let complexity = if max_symbols_per_file > 200 {
        format!("High: largest file has {max_symbols_per_file} symbols")
    } else if max_symbols_per_file > 100 {
        format!("Medium: largest file has {max_symbols_per_file} symbols")
    } else {
        format!("Low: largest file has {max_symbols_per_file} symbols")
    };

    ProjectConventions {
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
