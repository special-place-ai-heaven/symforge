//! Full symbol-context modes over one admitted publication.
use super::embed_query::{Budget, EmbeddedQuerySnapshot, symbol_row, validate_kind, validate_path};
use super::guidance::{
    knowledge_model, read_context, search_envelope, source, symbol_context as shared,
};
use crate::embed::parity::symbol_context::*;
use crate::embed::parity::{QueryOutput, QueryRefusalKind, QuerySymbol};
use crate::live_index::query::TraceSymbolFoundView;
use crate::live_index::{
    ContextBundleFoundView, ContextBundleReferenceView, ContextBundleSectionView,
    ContextBundleView, TraceSymbolView,
};

pub(super) fn validate(request: &SymbolContextRequest) -> Result<(), QueryRefusalKind> {
    if request.name.trim().is_empty()
        || request.symbol_line == Some(0)
        || request.max_tokens == Some(0)
    {
        return Err(QueryRefusalKind::InvalidRequest);
    }
    for path in [request.path.as_deref(), request.file.as_deref()]
        .into_iter()
        .flatten()
    {
        validate_path(path, false)?;
    }
    validate_kind(request.symbol_kind.as_deref())?;
    if (request.bundle == Some(true) || request.sections.is_some()) && request.path.is_none() {
        return Err(QueryRefusalKind::InvalidRequest);
    }
    if request
        .verbosity
        .as_deref()
        .is_some_and(|value| !matches!(value, "full" | "summary" | "signature" | "compact"))
        || request.sections.as_ref().is_some_and(|sections| {
            sections.iter().any(|section| {
                !matches!(
                    section.as_str(),
                    "dependents" | "siblings" | "implementations" | "git" | "knowledge"
                )
            })
        })
    {
        return Err(QueryRefusalKind::UnsupportedOption);
    }
    if crate::knowledge::guard_query(&request.name).is_err() {
        return Err(QueryRefusalKind::AdmissionUnavailable);
    }
    Ok(())
}

fn row(budget: &mut Budget, bytes: usize) -> bool {
    if budget.remaining_rows() <= 1 {
        budget.truncated = true;
        return false;
    }
    budget.row(bytes)
}

fn reference(entry: &ContextBundleReferenceView) -> ContextReference {
    ContextReference {
        display_name: entry.display_name.clone(),
        path: entry.file_path.clone(),
        line: entry.line_number,
        enclosing: entry.enclosing.clone(),
        occurrence_count: entry.occurrence_count as u64,
    }
}

fn bound_section(section: &mut ContextBundleSectionView, budget: &mut Budget) {
    let before = section.entries.len();
    section.entries.retain(|entry| {
        row(
            budget,
            super::embed_usage::decoded_bytes(&reference(entry)) as usize,
        )
    });
    section.overflow_count += before - section.entries.len();
    budget.truncated |= section.overflow_count > 0;
}

fn bound_context(
    snapshot: &EmbeddedQuerySnapshot,
    context: &mut ContextBundleFoundView,
    budget: &mut Budget,
) -> Result<(), QueryRefusalKind> {
    budget.required(context.body.len() + context.file_path.len() + context.kind_label.len())?;
    bound_section(&mut context.callers, budget);
    bound_section(&mut context.callees, budget);
    bound_section(&mut context.type_usages, budget);
    context.unresolved_same_name_member_calls.retain(|entry| {
        row(
            budget,
            super::embed_usage::decoded_bytes(&reference(entry)) as usize,
        )
    });
    let mut dependencies = Vec::new();
    for dependency in context.dependencies.drain(..) {
        super::embed_read::admit(snapshot, &dependency.file_path, None, budget)?;
        if row(
            budget,
            dependency.body.len()
                + dependency.name.len()
                + dependency.kind_label.len()
                + dependency.file_path.len(),
        ) {
            dependencies.push(dependency);
        }
    }
    context.dependencies = dependencies;
    context.implementation_suggestions.retain(|suggestion| {
        row(
            budget,
            suggestion.display_name.len() + suggestion.file_path.len(),
        )
    });
    Ok(())
}

fn section(section: &ContextBundleSectionView) -> ContextReferences {
    ContextReferences {
        total_count: section.total_count as u64,
        overflow_count: section.overflow_count as u64,
        unique_count: section.unique_count as u64,
        entries: section.entries.iter().map(reference).collect(),
    }
}

fn context(view: &ContextBundleFoundView) -> SymbolContext {
    SymbolContext {
        path: view.file_path.clone(),
        body: view.body.clone(),
        kind: view.kind_label.clone(),
        line_range: view.line_range,
        byte_count: view.byte_count as u64,
        callers: section(&view.callers),
        callees: section(&view.callees),
        type_usages: section(&view.type_usages),
        unresolved_same_name_member_calls: view
            .unresolved_same_name_member_calls
            .iter()
            .map(reference)
            .collect(),
        dependencies: view
            .dependencies
            .iter()
            .map(|dependency| ContextTypeDependency {
                name: dependency.name.clone(),
                kind: dependency.kind_label.clone(),
                path: dependency.file_path.clone(),
                line_range: dependency.line_range,
                body: dependency.body.clone(),
                depth: dependency.depth,
            })
            .collect(),
        implementation_suggestions: view
            .implementation_suggestions
            .iter()
            .map(|suggestion| ContextImplementationSuggestion {
                display_name: suggestion.display_name.clone(),
                path: suggestion.file_path.clone(),
                line: suggestion.line_number,
            })
            .collect(),
    }
}

fn trace(view: &mut TraceSymbolFoundView, budget: &mut Budget) -> SymbolTrace {
    use crate::embed::parity::reference::{DependentFile, DependentLine, Implementation};
    use crate::embed::parity::symbol::InspectionSymbol;
    view.siblings
        .retain(|sibling| row(budget, sibling.name.len() + sibling.kind_label.len()));
    view.implementations.entries.retain(|entry| {
        row(
            budget,
            entry.trait_name.len() + entry.implementor.len() + entry.file_path.len(),
        )
    });
    let mut dependents = Vec::new();
    for file in &mut view.dependents.files {
        file.lines.retain(|line| {
            row(
                budget,
                line.line_content.len() + line.name.len() + file.file_path.len(),
            )
        });
        if !file.lines.is_empty() {
            dependents.push(DependentFile {
                path: file.file_path.clone(),
                lines: file
                    .lines
                    .iter()
                    .map(|line| DependentLine {
                        line: line.line_number,
                        text: line.line_content.clone(),
                        kind: line.kind.clone(),
                        name: line.name.clone(),
                    })
                    .collect(),
            });
        }
    }
    view.dependents.files.retain(|file| !file.lines.is_empty());
    let git_activity = view.git_activity.as_mut().and_then(|git| {
        let bytes = git.churn_bar.len()
            + git.churn_label.len()
            + git.last_relative.len()
            + git.last_hash.len()
            + git.last_message.len()
            + git.last_author.len()
            + git.last_timestamp.len();
        if !row(budget, bytes) {
            return None;
        }
        git.owners.retain(|owner| row(budget, owner.len()));
        git.co_changes
            .retain(|(path, _, _)| row(budget, path.len()));
        Some(ContextGitActivity {
            churn_score: git.churn_score,
            churn_bar: git.churn_bar.clone(),
            churn_label: git.churn_label.clone(),
            commit_count: git.commit_count,
            last_relative: git.last_relative.clone(),
            last_hash: git.last_hash.clone(),
            last_message: git.last_message.clone(),
            last_author: git.last_author.clone(),
            last_timestamp: git.last_timestamp.clone(),
            owners: git.owners.clone(),
            co_changes: git.co_changes.clone(),
        })
    });
    if git_activity.is_none() {
        view.git_activity = None;
    }
    SymbolTrace {
        dependents,
        siblings: view
            .siblings
            .iter()
            .map(|sibling| InspectionSymbol {
                name: sibling.name.clone(),
                kind: sibling.kind_label.clone(),
                start_line: sibling.line_range.0,
                end_line: sibling.line_range.1,
            })
            .collect(),
        implementations: view
            .implementations
            .entries
            .iter()
            .map(|entry| Implementation {
                trait_name: entry.trait_name.clone(),
                implementor: entry.implementor.clone(),
                path: entry.file_path.clone(),
                line: entry.line,
            })
            .collect(),
        git_activity,
    }
}

fn candidates(
    snapshot: &EmbeddedQuerySnapshot,
    request: &SymbolContextRequest,
    budget: &mut Budget,
) -> Result<Vec<QuerySymbol>, QueryRefusalKind> {
    let mut candidates = Vec::new();
    for (_, file) in snapshot.generation.live.all_files() {
        if request
            .path
            .as_deref()
            .or(request.file.as_deref())
            .is_some_and(|path| path != file.relative_path)
        {
            continue;
        }
        for (index, symbol) in file.symbols.iter().enumerate() {
            if symbol.name != request.name
                || request
                    .symbol_line
                    .is_some_and(|line| line != symbol.line_range.0 + 1)
                || request.symbol_kind.as_deref().is_some_and(|kind| {
                    !crate::live_index::search::kind_filter_matches(kind, &symbol.kind)
                })
            {
                continue;
            }
            candidates.push(symbol_row(file, index, symbol)?);
        }
    }
    candidates.sort_by(|left, right| {
        (&left.path, left.byte_start, &left.kind).cmp(&(&right.path, right.byte_start, &right.kind))
    });
    let mut bounded = Vec::new();
    for candidate in candidates {
        if !row(
            budget,
            super::embed_usage::decoded_bytes(&candidate) as usize,
        ) {
            break;
        }
        bounded.push(candidate);
    }
    Ok(bounded)
}

pub(super) fn project(
    snapshot: &EmbeddedQuerySnapshot,
    request: &SymbolContextRequest,
    budget: &mut Budget,
) -> Result<QueryOutput, QueryRefusalKind> {
    budget.check()?;
    if request.project.as_deref().is_some_and(|project| {
        !snapshot
            .root
            .to_str()
            .is_some_and(|root| project == root || project == root.replace('\\', "/"))
    }) {
        return Err(QueryRefusalKind::AdmissionUnavailable);
    }
    let mode = if request.bundle == Some(true) {
        SymbolContextMode::Bundle
    } else if request.sections.is_some() {
        SymbolContextMode::Trace
    } else {
        SymbolContextMode::Default
    };
    let mut result = SymbolContextResult {
        mode,
        path: None,
        symbol: None,
        context: None,
        trace: None,
        rendered: String::new(),
        estimate: None,
        refusal: None,
        candidates: Vec::new(),
    };
    let path = if let Some(path) = request.path.as_ref().or(request.file.as_ref()) {
        path.clone()
    } else {
        let mut paths: Vec<_> = snapshot
            .generation
            .live
            .all_files()
            .filter(|(_, file)| {
                !matches!(
                    crate::live_index::query::resolve_symbol_selector(
                        file,
                        &request.name,
                        request.symbol_kind.as_deref(),
                        request.symbol_line,
                    ),
                    crate::live_index::query::SymbolSelectorMatch::NotFound
                )
            })
            .map(|(path, _)| path.to_string())
            .collect();
        paths.sort_unstable();
        if paths.len() != 1 {
            result.refusal = Some(if paths.is_empty() {
                QueryRefusalKind::NotFound
            } else {
                QueryRefusalKind::AmbiguousSymbol
            });
            result.candidates = candidates(snapshot, request, budget)?;
            result.rendered = budget.text(&if paths.is_empty() {
                format!("Symbol \"{}\" not found in index.", request.name)
            } else {
                shared::format_ambiguous_symbol_context(&request.name, &paths)
            })?;
            return Ok(QueryOutput::SymbolContext(result));
        }
        paths[0].clone()
    };
    result.path = Some(path.clone());
    super::embed_read::admit(snapshot, &path, None, budget)?;
    let published = snapshot.generation.as_ref();
    let file = published
        .live
        .get_file(&path)
        .ok_or(QueryRefusalKind::NotFound)?;
    let (index, symbol) = match crate::live_index::query::resolve_symbol_selector(
        file,
        &request.name,
        request.symbol_kind.as_deref(),
        request.symbol_line,
    ) {
        crate::live_index::query::SymbolSelectorMatch::Selected(index, symbol) => (index, symbol),
        crate::live_index::query::SymbolSelectorMatch::Ambiguous(lines) => {
            result.refusal = Some(QueryRefusalKind::AmbiguousSymbol);
            result.candidates = candidates(snapshot, request, budget)?;
            result.rendered=budget.text(&format!("Ambiguous symbol selector for {} in {}; pass symbol_line to disambiguate. Candidates: {:?}",request.name,path,lines))?;
            return Ok(QueryOutput::SymbolContext(result));
        }
        crate::live_index::query::SymbolSelectorMatch::NotFound => {
            return Err(QueryRefusalKind::NotFound);
        }
    };
    if request.estimate == Some(true) {
        let body_tokens = (symbol.byte_range.1 - symbol.byte_range.0) as u64 / 4;
        let caller_tokens = file
            .references
            .iter()
            .filter(|reference| reference.name == request.name)
            .count() as u64
            * 15
            + 50;
        result.estimate = Some(SymbolContextEstimate {
            body_tokens,
            caller_tokens,
            bundle_tokens: body_tokens * 3,
            raw_file_tokens: file.content.len() as u64 / 4,
        });
        result.rendered=budget.text(&format!("Estimate for get_symbol_context(name=\"{}\"):\n  Symbol body: ~{} tokens\n  Callers: ~{} tokens\n  Bundle: ~{} tokens",request.name,body_tokens,caller_tokens,body_tokens*3))?;
        return Ok(QueryOutput::SymbolContext(result));
    }
    let symbol = symbol_row(file, index, symbol)?;
    budget.charge_bytes(super::embed_usage::decoded_bytes(&symbol) as usize)?;
    result.symbol = Some(symbol);
    let knowledge_requested = request.sections.as_ref().is_none_or(|sections| {
        sections.is_empty() || sections.iter().any(|section| section == "knowledge")
    });
    let knowledge_only = request
        .sections
        .as_ref()
        .is_some_and(|sections| sections.len() == 1 && sections[0] == "knowledge");
    let knowledge = knowledge_model::resolve_symbol_code_anchor(
        published,
        &path,
        &request.name,
        request.symbol_kind.as_deref(),
        request.symbol_line,
    )
    .and_then(|target| {
        knowledge_model::render_code_knowledge_context(
            published,
            &target,
            mode == SymbolContextMode::Trace,
        )
    });
    if knowledge_only {
        let full = knowledge_model::render_budgeted_code_knowledge_only(
            published,
            knowledge.as_deref(),
            None,
        );
        result.rendered = full.clone();
        budget.cache_output = Some(QueryOutput::SymbolContext(result.clone()));
        budget.cache_truncated = budget.truncated;
        let rendered = knowledge_model::render_budgeted_code_knowledge_only(
            published,
            knowledge.as_deref(),
            request.max_tokens,
        );
        let reduced = rendered != full;
        let (rendered, bounded) = knowledge_model::bound_final_code_context_output_flagged(
            rendered,
            request.max_tokens,
            &format!("symbol:{path}:{}", request.name),
            request.sections.as_deref(),
            true,
            false,
            None,
        );
        budget.truncated |= reduced || bounded;
        result.rendered = budget.text(&rendered)?;
        return Ok(QueryOutput::SymbolContext(result));
    }
    let sections = request.sections.as_ref().map(|sections| {
        sections
            .iter()
            .filter(|section| section.as_str() != "knowledge")
            .cloned()
            .collect::<Vec<_>>()
    });
    let sections = sections.as_deref().filter(|sections| !sections.is_empty());
    let mut trace_view = (mode == SymbolContextMode::Trace).then(|| {
        shared::capture_trace_symbol_view(
            published,
            &path,
            &request.name,
            request.symbol_kind.as_deref(),
            request.symbol_line,
            sections,
            request.include_tests,
        )
    });
    let mut bundle = if let Some(TraceSymbolView::Found(found)) = &trace_view {
        ContextBundleView::Found(Box::new(found.context_bundle.clone()))
    } else {
        published.live.capture_context_bundle_view(
            &path,
            &request.name,
            request.symbol_kind.as_deref(),
            request.symbol_line,
        )
    };
    if let ContextBundleView::Found(found) = &mut bundle {
        bound_context(snapshot, found, budget)?;
        result.context = Some(context(found));
        if let Some(TraceSymbolView::Found(trace_found)) = &mut trace_view {
            trace_found.context_bundle = found.as_ref().clone();
            result.trace = Some(trace(trace_found, budget));
        }
    }
    let references = if mode == SymbolContextMode::Default {
        let selector = shared::SymbolContextSelector {
            name: &request.name,
            file: request.file.as_deref(),
            path: request.path.as_deref(),
            symbol_kind: request.symbol_kind.as_deref(),
            symbol_line: request.symbol_line,
        };
        let max_references = budget.remaining_rows().saturating_sub(1).min(10);
        let (text, _, count) = shared::symbol_context_references(
            published,
            &selector,
            4000,
            max_references,
            read_context::ContextSourceAuthority::CurrentIndex,
        );
        for _ in 0..count {
            budget.required(0)?;
        }
        Some(text)
    } else {
        None
    };
    let render = |max_tokens: Option<u64>| {
        let verbosity = request.verbosity.as_deref().unwrap_or("full");
        let mut render_truncated = false;
        let mut rendered = match mode {
            SymbolContextMode::Bundle => {
                let body = shared::context_bundle_result_view_with_max_tokens(
                    &bundle, verbosity, max_tokens,
                );
                let envelope = if let ContextBundleView::Found(found) = &bundle {
                    search_envelope::format_search_envelope(
                        if request.symbol_line.is_some() {
                            "exact"
                        } else {
                            "constrained"
                        },
                        search_envelope::SourceAuthority::from_freshness(&published.freshness),
                        read_context::parse_state_label(file),
                        &shared::context_bundle_completeness_label(found, &body),
                        &max_tokens
                            .map(|tokens| {
                                format!("path `{path}`; bundle mode; max_tokens={tokens}")
                            })
                            .unwrap_or_else(|| format!("path `{path}`; bundle mode")),
                        &format!("symbol anchor `{path}:{}`", found.line_range.0 + 1),
                    )
                } else {
                    String::new()
                };
                let text = format!("{envelope}\n\n{body}");
                format!(
                    "{text}{}",
                    read_context::compact_savings_footer(text.len(), file.content.len())
                )
            }
            SymbolContextMode::Trace if knowledge_only => {
                let text = knowledge_model::render_budgeted_code_knowledge_only(
                    published,
                    knowledge.as_deref(),
                    max_tokens,
                );
                render_truncated = max_tokens.is_some()
                    && text
                        != knowledge_model::render_budgeted_code_knowledge_only(
                            published,
                            knowledge.as_deref(),
                            None,
                        );
                text
            }
            SymbolContextMode::Trace => {
                let mut text = shared::trace_symbol_result_view(
                    trace_view.as_ref().expect("trace mode view"),
                    &request.name,
                    verbosity,
                    max_tokens,
                );
                let knowledge_start = if knowledge_requested {
                    knowledge.as_ref().map(|knowledge| {
                        text.push_str("\n\n");
                        let start = text.len();
                        text.push_str(knowledge);
                        start
                    })
                } else {
                    None
                };
                let (text, truncated) =
                    knowledge_model::enforce_budgeted_code_context_with_knowledge(
                        published,
                        text,
                        knowledge_start,
                        knowledge_start.and(knowledge.as_deref()),
                        max_tokens,
                    );
                render_truncated |= truncated;
                text
            }
            SymbolContextMode::Default => {
                let mut text = shared::render_symbol_context_header(
                    file,
                    &request.name,
                    request.symbol_kind.as_deref(),
                    request.symbol_line,
                    verbosity,
                    max_tokens,
                )
                .unwrap_or_default();
                text.push_str("\n\n");
                text.push_str(references.as_deref().unwrap_or(""));
                let callees = shared::context_bundle_callees_text(&bundle);
                if !callees.is_empty() {
                    text.push('\n');
                    text.push_str(&callees);
                }
                text.push_str(
                    shared::context_bundle_impl_suggestion_tip(&bundle).trim_start_matches('\n'),
                );
                let knowledge_start = knowledge.as_ref().map(|knowledge| {
                    text.push_str("\n\n");
                    let start = text.len();
                    text.push_str(knowledge);
                    start
                });
                let footer = read_context::compact_savings_footer(text.len(), file.content.len());
                let (text, truncated) =
                    knowledge_model::enforce_budgeted_code_context_with_knowledge(
                        published,
                        format!("{text}{footer}"),
                        knowledge_start,
                        knowledge.as_deref(),
                        max_tokens,
                    );
                render_truncated |= truncated;
                text
            }
        };
        render_truncated |= rendered.contains(source::CANONICAL_TRUNCATION_MARKER)
            || rendered.contains("[adaptive verbosity:");
        let withheld =
            read_context::withheld_not_searched_note(published.live.withheld_since_restore(), None);
        if let Some(withheld) = &withheld {
            rendered.push_str("\n\n");
            rendered.push_str(withheld);
        }
        let (rendered, final_truncated) = knowledge_model::bound_final_code_context_output_flagged(
            rendered,
            max_tokens,
            &format!("symbol:{path}:{}", request.name),
            request.sections.as_deref(),
            knowledge_requested,
            withheld.is_some(),
            None,
        );
        (rendered, render_truncated || final_truncated)
    };
    result.rendered = render(None).0;
    budget.cache_output = Some(QueryOutput::SymbolContext(result.clone()));
    budget.cache_truncated = budget.truncated;
    let effective_tokens = request
        .max_tokens
        .or_else(|| (mode == SymbolContextMode::Trace && knowledge_requested).then_some(1000));
    let (rendered, render_truncated) = render(effective_tokens);
    result.rendered = rendered;
    budget.truncated |= render_truncated;
    if budget.truncated {
        result.rendered = source::downgrade_full_completeness_after_truncation(&result.rendered);
    }
    result.rendered = budget.text(&result.rendered)?;
    Ok(QueryOutput::SymbolContext(result))
}
