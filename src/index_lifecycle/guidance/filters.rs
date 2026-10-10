//! Shared interpretation of language, path and pattern filters.

use crate::domain::index::LanguageId;
use crate::live_index::search;

pub(crate) fn parse_language_filter(input: Option<&str>) -> Result<Option<LanguageId>, String> {
    let Some(language) = input.map(str::trim).filter(|language| !language.is_empty()) else {
        return Ok(None);
    };

    let normalized = language.to_ascii_lowercase();
    let parsed = match normalized.as_str() {
        "rust" => Some(LanguageId::Rust),
        "python" => Some(LanguageId::Python),
        "javascript" => Some(LanguageId::JavaScript),
        "typescript" => Some(LanguageId::TypeScript),
        "go" => Some(LanguageId::Go),
        "java" => Some(LanguageId::Java),
        "c" => Some(LanguageId::C),
        "c++" => Some(LanguageId::Cpp),
        "c#" => Some(LanguageId::CSharp),
        "ruby" => Some(LanguageId::Ruby),
        "php" => Some(LanguageId::Php),
        "swift" => Some(LanguageId::Swift),
        "kotlin" => Some(LanguageId::Kotlin),
        "dart" => Some(LanguageId::Dart),
        "perl" => Some(LanguageId::Perl),
        "elixir" => Some(LanguageId::Elixir),
        "json" => Some(LanguageId::Json),
        "toml" => Some(LanguageId::Toml),
        "yaml" => Some(LanguageId::Yaml),
        "markdown" | "md" => Some(LanguageId::Markdown),
        "env" => Some(LanguageId::Env),
        "html" => Some(LanguageId::Html),
        "css" => Some(LanguageId::Css),
        "scss" => Some(LanguageId::Scss),
        _ => None,
    };

    parsed.map(Some).ok_or_else(|| {
        "Unsupported language filter. Use one of: Rust, Python, JavaScript, TypeScript, Go, Java, C, C++, C#, Ruby, PHP, Swift, Kotlin, Dart, Perl, Elixir, JSON, TOML, YAML, Markdown, Env, HTML, CSS, SCSS.".to_string()
    })
}

pub(crate) fn normalize_path_prefix(input: Option<&str>) -> search::PathScope {
    let Some(prefix) = input.map(str::trim).filter(|prefix| !prefix.is_empty()) else {
        return search::PathScope::any();
    };

    let normalized = prefix
        .replace('\\', "/")
        .trim_start_matches("./")
        .trim_start_matches('/')
        .trim_end_matches('/')
        .to_string();

    if normalized.is_empty() {
        search::PathScope::any()
    } else {
        search::PathScope::prefix(normalized)
    }
}

pub(crate) fn normalize_search_text_glob(input: Option<&str>) -> Option<String> {
    input
        .map(str::trim)
        .filter(|pattern| !pattern.is_empty())
        .map(|pattern| {
            let normalized = pattern
                .replace('\\', "/")
                .trim_start_matches("./")
                .trim_start_matches('/')
                .to_string();
            // Auto-prefix bare filenames (no glob chars, no path separators)
            // so "foo.rs" matches "**/foo.rs" instead of failing silently.
            if !normalized.contains('*')
                && !normalized.contains('/')
                && !normalized.contains('?')
                && !normalized.contains('[')
            {
                format!("**/{normalized}")
            } else {
                normalized
            }
        })
        .filter(|pattern| !pattern.is_empty())
}

/// Attempt to fix double-escaped regex character classes (e.g., `\\s` -> `\s`).
/// Returns `Some(fixed)` if the pattern contains likely double-escaped sequences,
/// `None` otherwise.
pub(crate) fn fix_common_double_escapes(pattern: &str) -> Option<String> {
    static RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"\\\\([sdwbntSDWB])").expect("static regex")
    });
    if !RE.is_match(pattern) {
        return None;
    }
    Some(RE.replace_all(pattern, r"\$1").to_string())
}
