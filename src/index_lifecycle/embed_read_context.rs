//! Repository orientation and file context over one admitted publication.

use super::embed_query::{Budget, EmbeddedQuerySnapshot, validate_path};
use super::embed_session::QuerySession;
use super::guidance::{file_read, knowledge_model, read_context, source};
use crate::embed::parity::read_context::{
    FileContextRequest, FileContextResult, RepoMapRequest, RepoMapResult,
};
use crate::embed::parity::{QueryOutput, QueryRefusalKind};
use crate::live_index::knowledge_bridge::CodeAnchorId;
use crate::live_index::query::RepoOutlineView;

fn bound_project(snapshot: &EmbeddedQuerySnapshot, selector: Option<&str>) -> bool {
    selector.is_none_or(|selector| {
        snapshot
            .root
            .to_str()
            .is_some_and(|root| selector == root || selector == root.replace('\\', "/"))
    })
}

pub(super) fn validate_repo_map(request: &RepoMapRequest) -> Result<(), QueryRefusalKind> {
    if let Some(path) = &request.path {
        validate_path(path, true)?;
    }
    if request.max_tokens == Some(0) {
        return Err(QueryRefusalKind::InvalidRequest);
    }
    Ok(())
}

pub(super) fn repo_map(
    snapshot: &EmbeddedQuerySnapshot,
    request: &RepoMapRequest,
    budget: &mut Budget,
) -> Result<QueryOutput, QueryRefusalKind> {
    budget.check()?;
    if !bound_project(snapshot, request.project.as_deref()) {
        return Err(QueryRefusalKind::AdmissionUnavailable);
    }
    budget.cap_tokens(request.max_tokens);
    let published = snapshot.generation.as_ref();
    let view = published.outline.as_ref();
    let file_count = published.live.file_count();
    let requested_detail = request.detail.as_deref().unwrap_or("compact");
    if request.estimate {
        let estimate = match requested_detail {
            "full" => (request.max_files.unwrap_or(200) as usize).min(file_count) * 30,
            "tree" => file_count * 10,
            _ => 500,
        };
        let rendered = format!(
            "Estimate for get_repo_map: ~{estimate} tokens (detail={requested_detail}, files={file_count})"
        );
        return Ok(QueryOutput::RepoMap(RepoMapResult {
            detail: requested_detail.to_string(),
            rendered: budget.text(&rendered)?,
            estimated_tokens: Some(estimate as u64),
            total_files: file_count as u64,
            shown_files: 0,
        }));
    }
    let detail = if let Some(detail) = request.detail.as_deref() {
        detail
    } else if let Some(max_tokens) = request.max_tokens {
        let full_estimate = (request.max_files.unwrap_or(200) as usize).min(file_count) * 30;
        if full_estimate as u64 <= max_tokens {
            "full"
        } else {
            "compact"
        }
    } else {
        "compact"
    };
    let project_name = snapshot
        .root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("project");
    let (mut rendered, selected_files) = match detail {
        "full" => {
            let files = view
                .files
                .iter()
                .filter(|file| {
                    request.path.as_ref().is_none_or(|path| file.relative_path.starts_with(path))
                })
                .cloned()
                .collect::<Vec<_>>();
            let max_files = request.max_files.unwrap_or(200) as usize;
            let shown = files.len().min(max_files);
            let truncated = files.len().saturating_sub(shown);
            let filtered = RepoOutlineView {
                total_files: files.len(),
                total_symbols: files.iter().map(|file| file.symbol_count).sum(),
                files: files.into_iter().take(shown).collect(),
            };
            let mut text = read_context::repo_outline_view(&filtered, project_name);
            if truncated > 0 {
                let hint = if request.path.is_some() {
                    "increase max_files= to see more"
                } else {
                    "use path= to scope or increase max_files="
                };
                text.push_str(&format!("\n\n... and {truncated} more files ({hint})"));
                budget.truncated = true;
            }
            (text, shown)
        }
        "tree" => {
            let path = request.path.as_deref().unwrap_or("");
            let depth = request.depth.unwrap_or(2).min(5);
            let skipped = published.live.compatibility_skipped_files();
            let shown = view
                .files
                .iter()
                .filter(|file| {
                    path.is_empty()
                        || file.relative_path == path
                        || file.relative_path.starts_with(&format!("{path}/"))
                })
                .count();
            (
                read_context::file_tree_view_with_skipped(&view.files, &skipped, path, depth),
                shown,
            )
        }
        _ => {
            let mut text = read_context::repo_map_text_for_generation(published);
            text.push_str("\nDoctrine: the map orients; the tools prove. Completeness: ranked and truncated by result cap - absence from the map is not absence from the repo; confirm with search_symbols / search_text before concluding something is missing.");
            (text, 0)
        }
    };
    let body_lines = rendered.lines().count();
    budget.check()?;
    if request.path.is_none() {
        rendered.push_str("\n\n");
        rendered.push_str(&knowledge_model::render_repository_knowledge_map(published));
    }
    if let Some(note) = read_context::withheld_not_searched_note(
        published.live.withheld_since_restore(),
        None,
    ) {
        rendered.push_str("\n\n");
        rendered.push_str(&note);
    }
    // Keep the full projection retrievable, while each delivered line consumes
    // one result row. Count only file rows that actually survived both caps.
    let full = RepoMapResult {
        detail: detail.to_string(),
        rendered: rendered.clone(),
        estimated_tokens: None,
        total_files: file_count as u64,
        shown_files: selected_files as u64,
    };
    budget.cache_output = Some(QueryOutput::RepoMap(full));
    budget.cache_truncated = budget.truncated;
    let mut served = String::new();
    let mut body_rows: usize = 0;
    let mut shown_files = 0;
    let mut truncated = false;
    let row_limit = budget.remaining_rows();
    let byte_limit = budget.remaining_bytes();
    for (row, line) in rendered.lines().enumerate() {
        let extra = line.len() + usize::from(row > 0);
        if row >= row_limit || served.len().saturating_add(extra) > byte_limit {
            truncated = true;
            break;
        }
        if row > 0 {
            served.push('\n');
        }
        served.push_str(line);
        if row < body_lines {
            body_rows += 1;
            if detail == "tree" && line.contains("]  (") {
                shown_files += 1;
            }
        }
    }
    if detail == "full" {
        shown_files = selected_files.min(body_rows.saturating_sub(1));
    }
    if served.is_empty() && !rendered.is_empty() {
        return Err(QueryRefusalKind::BudgetTooSmall);
    }
    budget.truncated |= truncated;
    let rendered = budget.text(&served)?;
    Ok(QueryOutput::RepoMap(RepoMapResult {
        detail: detail.to_string(),
        rendered,
        estimated_tokens: None,
        total_files: file_count as u64,
        shown_files: shown_files as u64,
    }))
}

pub(super) fn validate_file_context(
    request: &FileContextRequest,
) -> Result<(), QueryRefusalKind> {
    validate_path(&request.path, false)?;
    if request.max_tokens == Some(0) {
        return Err(QueryRefusalKind::InvalidRequest);
    }
    Ok(())
}

pub(super) fn file_context(
    snapshot: &EmbeddedQuerySnapshot,
    request: &FileContextRequest,
    session: Option<&QuerySession>,
    budget: &mut Budget,
) -> Result<QueryOutput, QueryRefusalKind> {
    budget.check()?;
    if !bound_project(snapshot, request.project.as_deref()) {
        return Err(QueryRefusalKind::AdmissionUnavailable);
    }
    super::embed_read::admit(snapshot, &request.path, None, budget)?;
    let published = snapshot.generation.as_ref();
    let file = published
        .live
        .get_file(&request.path)
        .ok_or(QueryRefusalKind::NotFound)?;
    let raw_chars = file.content.len();
    let raw_tokens = (raw_chars / 4) as u64;
    if request.estimate {
        let outline_tokens = (raw_chars / 50) as u64;
        let savings = (outline_tokens as usize * 100)
            .checked_div(raw_tokens as usize)
            .map(|pct| 100usize.saturating_sub(pct))
            .unwrap_or(0);
        let rendered = format!(
            "Estimate for get_file_context(path=\"{}\"):\n  Outline: ~{} tokens\n  Raw file: ~{} tokens\n  Savings: ~{}%",
            request.path, outline_tokens, raw_tokens, savings
        );
        return Ok(QueryOutput::FileContext(FileContextResult {
            path: request.path.clone(),
            rendered: budget.text(&rendered)?,
            estimated_tokens: Some(outline_tokens),
            raw_file_tokens: Some(raw_tokens),
            selected_sections: Vec::new(),
            cache_hit: false,
        }));
    }
    if let Some((handle, body)) = session.and_then(|session| {
        session.file_context_cache_hit(snapshot, request, budget.limits())
    }) {
        budget.charge_bytes(request.path.len())?;
        let rendered = budget.text(&body)?;
        budget.reused_handle = Some(handle);
        return Ok(QueryOutput::FileContext(FileContextResult {
            path: request.path.clone(),
            rendered,
            estimated_tokens: None,
            raw_file_tokens: Some(raw_tokens),
            selected_sections: request.sections.clone().unwrap_or_default(),
            cache_hit: true,
        }));
    }
    let line_count = read_context::indexed_file_line_count(&file.content);
    let is_small_file = read_context::is_small_indexed_file(raw_chars, line_count);
    let default_sections = request.sections.as_ref().is_none_or(Vec::is_empty);
    let large_default_summary = !is_small_file
        && default_sections
        && (raw_chars > 200_000 || file.symbols.len() > 250 || file.references.len() > 800);
    let knowledge_requested = request
        .sections
        .as_ref()
        .is_none_or(|sections| sections.is_empty() || sections.iter().any(|s| s == "knowledge"));
    let knowledge_only = request
        .sections
        .as_ref()
        .is_some_and(|sections| sections.len() == 1 && sections[0] == "knowledge");
    let sections = if is_small_file && default_sections {
        Some(vec!["outline".to_string()])
    } else if large_default_summary {
        Some(vec!["outline".to_string(), "imports".to_string()])
    } else {
        request.sections.as_ref().map(|sections| {
            sections
                .iter()
                .filter(|section| section.as_str() != "knowledge")
                .cloned()
                .collect::<Vec<_>>()
        })
    };
    let context_max_tokens = Some(file_read::resolve_read_max_tokens(
        request
            .max_tokens
            .or_else(|| knowledge_requested.then_some(1000))
            .or_else(|| large_default_summary.then_some(1000)),
        raw_chars,
    ));
    budget.cap_tokens(context_max_tokens);
    let knowledge = knowledge_requested
        .then(|| {
            knowledge_model::render_code_knowledge_context(
                published,
                &CodeAnchorId::File {
                    path: request.path.clone(),
                },
                true,
            )
        })
        .flatten();
    let mut truncated = false;
    let mut rendered = if knowledge_only {
        knowledge_model::render_budgeted_code_knowledge_only(
            published,
            knowledge.as_deref(),
            context_max_tokens,
        )
    } else {
        let params = read_context::OutlineProjectionParams {
            path: request.path.clone(),
            max_tokens: context_max_tokens,
            sections: sections.clone(),
        };
        let (text, _, _) = read_context::outline_text_for_generation(
            published,
            &params,
            false,
            read_context::ContextSourceAuthority::CurrentIndex,
        )
        .ok_or(QueryRefusalKind::NotFound)?;
        let mut body = read_context::collapse_large_test_modules(
            text,
            request.include_tests,
            sections.as_deref(),
        );
        if large_default_summary {
            let note = format!(
                "Large file summary: outline+imports only ({} symbols, {} references); omitted consumers/references/git; request sections=[\"consumers\"], sections=[\"references\"], sections=[\"git\"] or higher max_tokens.",
                file.symbols.len(), file.references.len()
            );
            body = if let Some((envelope, rest)) = body.split_once("\n\n") {
                format!("{envelope}\n\n{note}\n\n{rest}")
            } else {
                format!("{note}\n\n{body}")
            };
        }
        let code_context_len = body.len();
        let mut knowledge_start = None;
        if let Some(knowledge) = &knowledge {
            body.push_str("\n\n");
            knowledge_start = Some(body.len());
            body.push_str(knowledge);
        }
        let footer = read_context::compact_savings_footer(body.len(), raw_chars);
        let (output, limited) = if default_sections {
            knowledge_model::enforce_budgeted_code_context_prioritizing_code(
                published,
                format!("{body}{footer}"),
                code_context_len,
                knowledge.as_deref(),
                context_max_tokens,
            )
        } else {
            knowledge_model::enforce_budgeted_code_context_with_knowledge(
                published,
                format!("{body}{footer}"),
                knowledge_start,
                knowledge.as_deref(),
                context_max_tokens,
            )
        };
        truncated |= limited;
        output
    };
    let project_wide = request.sections.as_ref().is_none_or(|sections| {
        sections.is_empty()
            || sections
                .iter()
                .any(|section| section == "consumers" || section == "references")
    });
    let withheld_note = project_wide
        .then(|| read_context::withheld_not_searched_note(published.live.withheld_since_restore(), None))
        .flatten();
    if truncated {
        let mixed_knowledge_sections = request.sections.as_ref().is_some_and(|sections| {
            sections.iter().any(|section| section == "knowledge")
                && sections.iter().any(|section| section != "knowledge")
        });
        if mixed_knowledge_sections
            && !rendered.contains("Requested code section(s)")
            && !rendered.contains("Requested code and knowledge sections")
            && !rendered.contains("Code section omitted")
        {
            rendered.push_str("\n\nRequested code and knowledge sections may be incomplete at this budget; retry the same sections with a higher max_tokens.");
        }
        rendered = source::downgrade_full_completeness_after_truncation(&rendered);
    }
    if let Some(note) = &withheld_note {
        rendered.push_str("\n\n");
        rendered.push_str(note);
    }
    if request.force_refresh {
        if let Some(prior) = session.and_then(|session| {
            session.prior_file_context_fetch(snapshot, request, budget.limits())
        }) {
            rendered = file_read::append_dedup_hint_footer(
                rendered,
                "file context",
                prior.fetched_at.elapsed().as_secs(),
                prior.approx_tokens,
            );
        }
    }
    let full = FileContextResult {
        path: request.path.clone(),
        rendered: rendered.clone(),
        estimated_tokens: None,
        raw_file_tokens: Some(raw_tokens),
        selected_sections: sections.clone().unwrap_or_default(),
        cache_hit: false,
    };
    budget.charge_bytes(
        full.path.len() + full.selected_sections.iter().map(String::len).sum::<usize>(),
    )?;
    budget.cache_output = Some(QueryOutput::FileContext(full));
    budget.cache_truncated = truncated;
    let full_len = rendered.len();
    let mut bounded = knowledge_model::bound_final_code_context_output(
        rendered,
        context_max_tokens,
        &format!("file:{}", request.path),
        request.sections.as_deref(),
        knowledge_requested,
        withheld_note.is_some(),
        None,
    );
    if bounded.len() < full_len || bounded.len() > budget.remaining_bytes() {
        bounded = source::downgrade_full_completeness_after_truncation(&bounded);
    }
    budget.truncated |= truncated || bounded.len() < full_len;
    let rendered = budget.text(&bounded)?;
    Ok(QueryOutput::FileContext(FileContextResult {
        path: request.path.clone(),
        rendered,
        estimated_tokens: None,
        raw_file_tokens: Some(raw_tokens),
        selected_sections: sections.unwrap_or_default(),
        cache_hit: false,
    }))
}
