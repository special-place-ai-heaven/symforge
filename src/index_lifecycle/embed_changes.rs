//! Native `what_changed` and `diff_symbols` over one captured publication.
//!
//! Mode selection, filters, envelopes, the symbol-delta engine and every
//! repository read come from the same shared modules the MCP handlers call;
//! git objects and working-tree bytes pass the shared read gate.

use std::path::Path;

use super::embed_query::{Budget, EmbeddedQuerySnapshot, validate_path};
use super::guidance::changes::{self as shared, WhatChangedMode, WhatChangedOptions};
use super::guidance::{read_gate, search_envelope};
use crate::embed::parity::changes::{
    ChangeMode, DiffSymbolsRequest, DiffSymbolsResult, SymbolFileDelta, WhatChangedRequest,
    WhatChangedResult,
};
use crate::embed::parity::{QueryOutput, QueryRefusalKind};
use crate::live_index::LiveIndex;

/// Refuse a project selector that does not name this source's root. An embed
/// handle serves exactly one source, so any other selector is foreign.
pub(super) fn check_project(
    snapshot: &EmbeddedQuerySnapshot,
    project: Option<&str>,
) -> Result<(), QueryRefusalKind> {
    if project.is_some_and(|selected| {
        !snapshot
            .root
            .to_str()
            .is_some_and(|root| selected == root || selected == root.replace('\\', "/"))
    }) {
        return Err(QueryRefusalKind::AdmissionUnavailable);
    }
    Ok(())
}

/// The repository whose work tree IS this source root. Discovery that lands
/// on an enclosing repository would report paths relative to a different
/// root, so it is treated as "no repository".
pub(super) fn open_repository(root: &Path) -> Option<crate::git::GitRepo> {
    let repo = crate::git::GitRepo::open(root).ok()?;
    let workdir = std::fs::canonicalize(repo.workdir()?).ok()?;
    (workdir == std::fs::canonicalize(root).ok()?).then_some(repo)
}

/// Gated working-tree (`reference` empty) or git-object text, the same pair
/// of reads `diff_symbols_result_view` performs for MCP.
pub(super) fn gated_text(
    live: &LiveIndex,
    repo: &crate::git::GitRepo,
    reference: &str,
    path: &str,
) -> Result<Option<String>, String> {
    let mut ignore = |_| {};
    if reference.is_empty() {
        read_gate::worktree_text_with(repo, path, &mut |full_path| {
            read_gate::disk_read_with(live, path, full_path, &mut || None, &mut ignore)
        })
    } else {
        read_gate::admit_git_text_with(live, repo, reference, path, &mut ignore)
    }
}

fn validate_filters(
    path_prefix: Option<&str>,
    language: Option<&str>,
    max_tokens: Option<u64>,
) -> Result<(), QueryRefusalKind> {
    if max_tokens == Some(0) {
        return Err(QueryRefusalKind::InvalidRequest);
    }
    if let Some(prefix) = path_prefix {
        validate_path(prefix, true)?;
    }
    super::guidance::filters::parse_language_filter(language)
        .map_err(|_| QueryRefusalKind::UnsupportedOption)?;
    Ok(())
}

pub(super) fn validate_what_changed(request: &WhatChangedRequest) -> Result<(), QueryRefusalKind> {
    validate_filters(
        request.path_prefix.as_deref(),
        request.language.as_deref(),
        request.max_tokens,
    )
}

pub(super) fn validate_diff_symbols(request: &DiffSymbolsRequest) -> Result<(), QueryRefusalKind> {
    if [&request.base, &request.target]
        .into_iter()
        .any(|reference| reference.as_deref().is_some_and(|r| r.trim().is_empty()))
    {
        return Err(QueryRefusalKind::InvalidRequest);
    }
    validate_filters(
        request.path_prefix.as_deref(),
        request.language.as_deref(),
        request.max_tokens,
    )
}

/// A token cap bounds what is returned, never what is retrievable: the full
/// projection is cached before the cap applies.
fn cache_before_token_cap(
    budget: &mut Budget,
    max_tokens: Option<u64>,
    project: impl Fn(&mut Budget) -> Result<QueryOutput, QueryRefusalKind>,
) -> Result<QueryOutput, QueryRefusalKind> {
    if max_tokens.is_some() {
        let mut full_budget = budget.projection_budget();
        budget.cache_output = Some(project(&mut full_budget)?);
        budget.cache_truncated = full_budget.truncated;
        budget.check()?;
        budget.cap_tokens(max_tokens);
    }
    project(budget)
}

fn strings(values: &[String], budget: &mut Budget) -> Vec<String> {
    let mut rows = Vec::new();
    for value in values {
        if !budget.row(value.len()) {
            break;
        }
        rows.push(value.clone());
    }
    rows
}

/// The uncapped report, rendered exactly as the MCP handler renders it.
struct WhatChangedReport {
    mode: ChangeMode,
    paths: Vec<String>,
    total_paths: u64,
    symbol_diff: Option<(shared::SymbolDiffView, String)>,
    rendered: String,
}

pub(super) fn what_changed(
    snapshot: &EmbeddedQuerySnapshot,
    request: &WhatChangedRequest,
    budget: &mut Budget,
) -> Result<QueryOutput, QueryRefusalKind> {
    check_project(snapshot, request.project.as_deref())?;
    let options = WhatChangedOptions {
        since: request.since,
        git_ref: request.git_ref.as_deref(),
        uncommitted: request.uncommitted,
        path_prefix: request.path_prefix.as_deref(),
        language: request.language.as_deref(),
        code_only: request.code_only,
        include_symbol_diff: request.include_symbol_diff,
    };
    let repo = open_repository(&snapshot.root);
    let mode = shared::determine_what_changed_mode(&options, repo.is_some())
        .map_err(|_| QueryRefusalKind::GitUnavailable)?;
    let change_mode = match &mode {
        WhatChangedMode::Timestamp(since) => ChangeMode::Timestamp { since: *since },
        WhatChangedMode::GitRef(reference) => ChangeMode::GitRef {
            reference: reference.clone(),
        },
        WhatChangedMode::Uncommitted => ChangeMode::Uncommitted,
    };
    if request.estimate == Some(true) {
        let (tokens, rendered) =
            shared::what_changed_estimate(request.include_symbol_diff.unwrap_or(false));
        let rendered = budget.text(&rendered)?;
        return Ok(QueryOutput::WhatChanged(WhatChangedResult {
            mode: change_mode,
            paths: Vec::new(),
            total_paths: 0,
            filtered_paths: 0,
            symbol_diff: None,
            rendered,
            estimated_tokens: Some(tokens),
        }));
    }
    let report = what_changed_report(snapshot, &options, &mode, change_mode, repo.as_ref())?;
    budget.check()?;
    cache_before_token_cap(budget, request.max_tokens, |budget| {
        let rendered = budget.text(&report.rendered)?;
        let paths = strings(&report.paths, budget);
        let symbol_diff = match &report.symbol_diff {
            Some((view, rendered)) => Some(project_symbol_diff(view, rendered, None, budget)?),
            None => None,
        };
        Ok(QueryOutput::WhatChanged(WhatChangedResult {
            mode: report.mode.clone(),
            filtered_paths: report.paths.len() as u64,
            paths,
            total_paths: report.total_paths,
            symbol_diff,
            rendered,
            estimated_tokens: None,
        }))
    })
}

fn what_changed_report(
    snapshot: &EmbeddedQuerySnapshot,
    options: &WhatChangedOptions<'_>,
    mode: &WhatChangedMode,
    change_mode: ChangeMode,
    repo: Option<&crate::git::GitRepo>,
) -> Result<WhatChangedReport, QueryRefusalKind> {
    let live = snapshot.generation.live.as_ref();
    let filter = |paths: Vec<String>, code_only: bool| {
        shared::filter_paths_by_prefix_and_language(
            paths,
            options.path_prefix,
            options.language,
            code_only,
        )
        .map_err(|_| QueryRefusalKind::UnsupportedOption)
    };
    let envelope = |match_type: &str, parse_state: &str, total: usize, filtered: &[String]| {
        search_envelope::format_search_envelope(
            match_type,
            shared::what_changed_source_authority(mode, &snapshot.generation.freshness),
            parse_state,
            &shared::changed_paths_completeness_label(total, filtered.len()),
            &shared::what_changed_scope_summary(options, mode),
            &shared::search_paths_evidence(filtered.iter().map(String::as_str)),
        )
    };
    let message = |total_paths: usize, rendered: String| WhatChangedReport {
        mode: change_mode.clone(),
        paths: Vec::new(),
        total_paths: total_paths as u64,
        symbol_diff: None,
        rendered,
    };
    let include_symbol_diff = options.include_symbol_diff.unwrap_or(false);
    match mode {
        WhatChangedMode::Timestamp(since_ts) => {
            let view = live.capture_what_changed_timestamp_view();
            if *since_ts >= view.loaded_secs {
                return Ok(message(
                    view.paths.len(),
                    shared::what_changed_timestamp_view(&view, *since_ts),
                ));
            }
            let filtered = filter(view.paths.clone(), options.code_only.unwrap_or(false))?;
            if filtered.is_empty() {
                return Ok(message(
                    view.paths.len(),
                    if view.paths.is_empty() {
                        "Index is empty — no files tracked.".to_string()
                    } else {
                        "No indexed files matched the requested filters since the last index load."
                            .to_string()
                    },
                ));
            }
            let envelope = search_envelope::format_search_envelope(
                "exact (timestamp compare)",
                shared::what_changed_source_authority(mode, &snapshot.generation.freshness),
                super::guidance::reference_read::search_parse_state_for_paths(
                    live,
                    filtered.iter().map(String::as_str),
                ),
                &shared::changed_paths_completeness_label(view.paths.len(), filtered.len()),
                &shared::what_changed_scope_summary(options, mode),
                &shared::search_paths_evidence(filtered.iter().map(String::as_str)),
            );
            let output = shared::what_changed_paths_result(
                &filtered,
                "No indexed files matched the requested filters since the last index load.",
            );
            Ok(WhatChangedReport {
                mode: change_mode,
                total_paths: view.paths.len() as u64,
                paths: filtered,
                symbol_diff: None,
                rendered: format!("{envelope}\n\n{output}"),
            })
        }
        WhatChangedMode::Uncommitted | WhatChangedMode::GitRef(_) => {
            let repo = repo.ok_or(QueryRefusalKind::GitUnavailable)?;
            let (paths, code_only, match_type, empty, base, target) = match mode {
                WhatChangedMode::GitRef(git_ref) => (
                    repo.changed_paths_from_ref(git_ref),
                    options.code_only.unwrap_or(false),
                    "exact (git ref diff)",
                    format!("No changes detected relative to git ref '{git_ref}'."),
                    git_ref.as_str(),
                    "HEAD",
                ),
                _ => (
                    repo.uncommitted_paths(),
                    // US1 (018) FR-001/SC-001: uncommitted mode defaults to
                    // source-focused so untracked data files don't dominate.
                    options.code_only.unwrap_or(true),
                    "exact (uncommitted working tree)",
                    "No uncommitted changes detected.".to_string(),
                    "HEAD",
                    "",
                ),
            };
            let paths = paths.map_err(|_| QueryRefusalKind::GitUnavailable)?;
            let total_paths = paths.len();
            let filtered = filter(paths, code_only)?;
            if filtered.is_empty() {
                let rendered = match mode {
                    _ if total_paths == 0 => empty,
                    WhatChangedMode::GitRef(git_ref) => format!(
                        "No changes relative to git ref '{git_ref}' matched the requested filters."
                    ),
                    _ => {
                        // An empty filtered result must disclose the (default)
                        // source-focus filter and its recovery lever.
                        let default_note = if options.code_only.is_none() {
                            " Uncommitted mode is source-focused by default (code_only=true);"
                        } else {
                            ""
                        };
                        format!(
                            "No uncommitted changes matched the requested filters.{default_note} \
                             {total_paths} changed path(s) were filtered out — pass \
                             code_only=false (or widen path_prefix/language) to see them."
                        )
                    }
                };
                return Ok(message(total_paths, rendered));
            }
            let envelope = envelope(
                match_type,
                shared::what_changed_parse_state_label(mode, include_symbol_diff),
                total_paths,
                &filtered,
            );
            let mut output = shared::what_changed_paths_result(&filtered, &empty);
            let symbol_diff = include_symbol_diff.then(|| {
                let refs: Vec<&str> = filtered.iter().map(String::as_str).collect();
                let view = shared::capture_symbol_diff(base, target, &refs, |reference, path| {
                    gated_text(live, repo, reference, path)
                });
                let rendered = shared::render_symbol_diff(&view, true, false);
                output.push_str("\n\n");
                output.push_str(&rendered);
                (view, rendered)
            });
            Ok(WhatChangedReport {
                mode: change_mode,
                total_paths: total_paths as u64,
                paths: filtered,
                symbol_diff,
                rendered: format!("{envelope}\n\n{output}"),
            })
        }
    }
}

/// Project a shared symbol delta into its bounded public form.
fn project_symbol_diff(
    view: &shared::SymbolDiffView,
    rendered: &str,
    commits: Option<(Option<String>, Option<String>, u64)>,
    budget: &mut Budget,
) -> Result<DiffSymbolsResult, QueryRefusalKind> {
    let rendered = budget.text(rendered)?;
    let mut files = Vec::new();
    for file in &view.files {
        let size = file.path.len()
            + file.withheld.as_ref().map_or(0, String::len)
            + [&file.added, &file.removed, &file.modified]
                .into_iter()
                .flatten()
                .map(String::len)
                .sum::<usize>();
        if !budget.row(size) {
            break;
        }
        files.push(SymbolFileDelta {
            path: file.path.clone(),
            added: file.added.clone(),
            removed: file.removed.clone(),
            modified: file.modified.clone(),
            withheld: file.withheld.clone(),
        });
    }
    let (base_commit, target_commit, total) =
        commits.unwrap_or((None, None, view.changed_files as u64));
    Ok(DiffSymbolsResult {
        base: view.base.clone(),
        target: view.target.clone(),
        base_commit,
        target_commit,
        total_changed_files: total,
        filtered_changed_files: view.changed_files as u64,
        files,
        added_count: view.added_count as u64,
        removed_count: view.removed_count as u64,
        modified_count: view.modified_count as u64,
        withheld_count: view.withheld_count as u64,
        rendered,
        estimated_tokens: None,
    })
}

pub(super) fn diff_symbols(
    snapshot: &EmbeddedQuerySnapshot,
    request: &DiffSymbolsRequest,
    budget: &mut Budget,
) -> Result<QueryOutput, QueryRefusalKind> {
    check_project(snapshot, request.project.as_deref())?;
    let base = request.base.as_deref().unwrap_or("main");
    let target = request.target.as_deref().unwrap_or("HEAD");
    let compact = request.compact.unwrap_or(false);
    let summary_only = request.summary_only.unwrap_or(false);
    if request.estimate == Some(true) {
        let (tokens, rendered) = shared::diff_symbols_estimate(compact, summary_only);
        let rendered = budget.text(&rendered)?;
        return Ok(QueryOutput::DiffSymbols(DiffSymbolsResult {
            base: base.to_owned(),
            target: target.to_owned(),
            base_commit: None,
            target_commit: None,
            total_changed_files: 0,
            filtered_changed_files: 0,
            files: Vec::new(),
            added_count: 0,
            removed_count: 0,
            modified_count: 0,
            withheld_count: 0,
            rendered,
            estimated_tokens: Some(tokens),
        }));
    }
    let live = snapshot.generation.live.as_ref();
    let repo = open_repository(&snapshot.root).ok_or(QueryRefusalKind::GitUnavailable)?;
    let changed = repo
        .changed_paths_between_refs(base, target)
        .map_err(|_| QueryRefusalKind::GitUnavailable)?;
    let code_only = request.code_only.unwrap_or(false);
    let filtered = shared::filter_diff_symbol_paths(
        &changed,
        request.path_prefix.as_deref(),
        request.language.as_deref(),
        code_only,
    )
    .map_err(|_| QueryRefusalKind::UnsupportedOption)?;
    let commits = Some((
        repo.resolve_ref_commit(base).map(|oid| oid.to_string()),
        repo.resolve_ref_commit(target).map(|oid| oid.to_string()),
        changed.len() as u64,
    ));
    let (view, rendered) = if filtered.is_empty() {
        (
            shared::capture_symbol_diff(base, target, &[], |_, _| Ok(None)),
            format!("No file changes found between {base} and {target}."),
        )
    } else {
        let view = shared::capture_symbol_diff(base, target, &filtered, |reference, path| {
            gated_text(live, &repo, reference, path)
        });
        let envelope = shared::diff_symbols_envelope(
            base,
            target,
            changed.len(),
            &filtered,
            compact,
            summary_only,
            request.path_prefix.as_deref(),
            request.language.as_deref(),
            code_only,
        );
        let body = shared::render_symbol_diff(&view, compact, summary_only);
        (view, format!("{envelope}\n\n{body}"))
    };
    budget.check()?;
    cache_before_token_cap(budget, request.max_tokens, |budget| {
        project_symbol_diff(&view, &rendered, commits.clone(), budget).map(QueryOutput::DiffSymbols)
    })
}
