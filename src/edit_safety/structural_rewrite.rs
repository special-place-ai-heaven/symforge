//! `batch_edit` structural mode: rewrite every ast-grep match of a pattern
//! across the indexed files in scope. Both edit lanes plan through
//! [`plan_structural_rewrite`] and then stage, verify and commit the planned
//! images through their ordinary batch pipeline, so a dry run and a real run
//! read the same plan.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::index::LanguageId;
use crate::live_index::LiveIndex;

// The request. Plain `//` comments on purpose: doc comments would become
// schema descriptions, and the full tools/list surface has a byte budget.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[schemars(rename = "Rewrite")]
pub struct StructuralRewrite {
    // ast-grep pattern; `$X` captures one node, `$$$X` a sequence.
    pub pattern: String,
    // Replacement text; ast-grep substitutes the pattern's metavariables.
    pub template: String,
    // Canonical language name; default every structurally searchable one.
    #[serde(default)]
    pub language: Option<String>,
    // Repository-relative directory or file the rewrite is confined to.
    #[serde(default)]
    pub path_prefix: Option<String>,
    // Repository-relative include glob.
    #[serde(default)]
    pub glob: Option<String>,
    // Most match sites the call may rewrite (default 100, at most 1000).
    #[serde(default)]
    pub limit: Option<u32>,
}

/// Match sites one call rewrites when `limit` is omitted.
pub const DEFAULT_REWRITE_LIMIT: usize = 100;
/// The largest `limit` a call may ask for.
pub const MAX_REWRITE_LIMIT: usize = 1000;

/// One file the rewrite changes: its indexed bytes and the rewritten bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewrittenFile {
    pub path: String,
    pub language: LanguageId,
    pub original: Vec<u8>,
    pub replacement: Vec<u8>,
    pub sites: usize,
}

/// A planned structural rewrite: every changed file, in path order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewritePlan {
    pub files: Vec<RewrittenFile>,
}

impl RewritePlan {
    pub fn sites(&self) -> usize {
        self.files.iter().map(|file| file.sites).sum()
    }

    /// The per-file summary lines, `[DRY RUN] Would ` prefixed for a preview.
    pub fn summaries(&self, dry_run: bool) -> Vec<String> {
        self.files
            .iter()
            .map(|file| {
                let summary = format!(
                    "{} — rewrote {} structural match(es) ({} → {} bytes)",
                    file.path,
                    file.sites,
                    file.original.len(),
                    file.replacement.len()
                );
                if dry_run {
                    format!("[DRY RUN] Would {summary}")
                } else {
                    summary
                }
            })
            .collect()
    }

    /// The evidence line both lanes print.
    pub fn evidence(&self) -> String {
        format!(
            "{} structural match(es) across {} file(s)",
            self.sites(),
            self.files.len()
        )
    }
}

/// The refusal both lanes print, word for word.
pub fn refusal(message: impl std::fmt::Display) -> String {
    format!("Error: structural rewrite refused: {message}. No files were modified.")
}

/// Plan `request` over every indexed file in its scope. Refuses, with a reason
/// the caller can act on, when the pattern does not compile, the template names
/// an unknown metavariable, two matches overlap, nothing matches, more sites
/// match than `limit`, a candidate could not be parsed, or the rewrite would
/// leave any file with more syntax errors than it has now. `language` is
/// `request.language` already parsed by the lane's shared language filter
/// (its error goes through [`refusal`]).
pub fn plan_structural_rewrite(
    index: &LiveIndex,
    request: &StructuralRewrite,
    language: Option<LanguageId>,
) -> Result<RewritePlan, String> {
    if request.pattern.trim().is_empty() {
        return Err(refusal("`pattern` is empty"));
    }
    let limit = match request.limit {
        None => DEFAULT_REWRITE_LIMIT,
        Some(0) => return Err(refusal("`limit` must be at least 1")),
        Some(limit) if limit as usize > MAX_REWRITE_LIMIT => {
            return Err(refusal(format_args!(
                "`limit` {limit} exceeds the maximum of {MAX_REWRITE_LIMIT}"
            )));
        }
        Some(limit) => limit as usize,
    };
    let prefix = scope_prefix(request.path_prefix.as_deref())?;
    let glob = match request.glob.as_deref().filter(|glob| !glob.is_empty()) {
        None => None,
        Some(glob) => Some(
            globset::GlobBuilder::new(glob)
                .literal_separator(true)
                .build()
                .map_err(|error| refusal(format_args!("invalid `glob` '{glob}': {error}")))?
                .compile_matcher(),
        ),
    };
    if let Some(language) = language
        && crate::parsing::ast_grep::SgLang::from_language_id(&language, false).is_none()
    {
        return Err(refusal(format_args!(
            "{} has no structural grammar",
            language.name()
        )));
    }

    let mut candidates: Vec<(&String, &crate::live_index::store::IndexedFile)> = index
        .all_files()
        .filter(|(path, file)| {
            prefix.matches(path)
                && glob
                    .as_ref()
                    .is_none_or(|glob| glob.is_match(path.as_str()))
                && language.is_none_or(|language| file.language == language)
                && crate::parsing::ast_grep::SgLang::from_language_id(&file.language, false)
                    .is_some()
        })
        .collect();
    candidates.sort_by(|left, right| left.0.cmp(right.0));

    let mut compiled = std::collections::HashMap::new();
    let mut first_compile_error: Option<String> = None;
    let mut searched = 0usize;
    let mut files = Vec::new();
    let mut sites = 0usize;
    for (path, file) in candidates {
        let is_tsx = LanguageId::is_tsx_path(path);
        let pattern = compiled.entry((file.language, is_tsx)).or_insert_with(|| {
            crate::parsing::ast_grep::compile_structural_pattern(
                &request.pattern,
                &file.language,
                is_tsx,
            )
        });
        let pattern = match pattern.as_ref() {
            Ok(pattern) => pattern,
            Err(error) => {
                first_compile_error.get_or_insert_with(|| error.clone());
                continue;
            }
        };
        let Ok(source) = std::str::from_utf8(&file.content) else {
            continue;
        };
        searched += 1;
        let (rewritten, found) = crate::parsing::ast_grep::structural_rewrite_with_compiled(
            source,
            pattern,
            &request.template,
        )
        .map_err(|error| refusal(format_args!("{path}: {error}")))?;
        if found == 0 {
            continue;
        }
        sites += found;
        if sites > limit {
            return Err(refusal(format_args!(
                "more than {limit} match(es) in scope; narrow `path_prefix`, `glob` or `language`, or raise `limit`"
            )));
        }
        files.push(RewrittenFile {
            path: path.clone(),
            language: file.language,
            original: file.content.clone(),
            replacement: rewritten.into_bytes(),
            sites: found,
        });
    }
    if searched == 0 {
        return Err(refusal(match first_compile_error {
            Some(error) => error,
            None => "no indexed source file with a structural grammar is in scope".to_string(),
        }));
    }
    if files.is_empty() {
        return Err(refusal(format_args!(
            "the pattern matched nothing in {searched} file(s) in scope"
        )));
    }
    refuse_syntax_regressions(&files)?;
    Ok(RewritePlan { files })
}

/// The scope a `path_prefix` names. Absolute and parent-relative prefixes are
/// refused: a rewrite never reaches outside the repository.
fn scope_prefix(prefix: Option<&str>) -> Result<crate::live_index::search::PathScope, String> {
    let Some(prefix) = prefix.map(str::trim).filter(|prefix| !prefix.is_empty()) else {
        return Ok(crate::live_index::search::PathScope::any());
    };
    let normalized = prefix.replace('\\', "/");
    let normalized = normalized.trim_start_matches("./");
    let escapes = std::path::Path::new(prefix).is_absolute()
        || normalized.starts_with('/')
        || normalized.split('/').any(|part| part == "..");
    if escapes {
        return Err(refusal(format_args!(
            "`path_prefix` '{prefix}' is outside the repository; pass a repository-relative path"
        )));
    }
    Ok(crate::live_index::search::PathScope::prefix(normalized))
}

/// Refuse the plan when any rewritten file parses with more ERROR/MISSING
/// nodes than its current bytes, naming every such file.
fn refuse_syntax_regressions(files: &[RewrittenFile]) -> Result<(), String> {
    let mut broken = Vec::new();
    for file in files {
        let is_tsx = LanguageId::is_tsx_path(&file.path);
        let count = |bytes: &[u8]| {
            std::str::from_utf8(bytes)
                .map_err(|_| "invalid UTF-8".to_string())
                .and_then(|source| {
                    crate::parsing::syntax_error_count(source, &file.language, is_tsx)
                })
        };
        let before = count(&file.original)
            .map_err(|error| refusal(format_args!("{}: {error}", file.path)))?;
        let after = count(&file.replacement)
            .map_err(|error| refusal(format_args!("{}: {error}", file.path)))?;
        if after > before {
            broken.push(format!(
                "{} ({before} → {after} syntax error node(s))",
                file.path
            ));
        }
    }
    if broken.is_empty() {
        return Ok(());
    }
    Err(refusal(format_args!(
        "the template would break the syntax of {}",
        broken.join(", ")
    )))
}
