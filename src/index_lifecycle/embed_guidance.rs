//! Bounded public projections of the same guidance used by MCP.

use super::embed_query::{Budget, text_line};
use crate::embed::parity::guidance::{
    Conventions, EditPlanGuidance, Exploration, ExploreRequest, ExploreSymbol, RelatedFile,
};
use crate::embed::parity::search::SearchEstimate;
use crate::embed::parity::{QueryOutput, QueryRefusalKind};
use crate::live_index::LiveIndex;

fn strings(values: impl IntoIterator<Item = String>, budget: &mut Budget) -> Vec<String> {
    let mut rows = Vec::new();
    for value in values {
        if !budget.row(value.len()) {
            break;
        }
        rows.push(value);
    }
    rows
}

pub(super) fn conventions(
    live: &LiveIndex,
    budget: &mut Budget,
) -> Result<QueryOutput, QueryRefusalKind> {
    budget.check()?;
    let result = super::guidance::conventions::detect_conventions(live);
    budget.check()?;
    budget.required(
        result.language.len()
            + result.error_handling.len()
            + result.naming.len()
            + result.test_patterns.len()
            + result.file_organization.len()
            + result.complexity.len(),
    )?;
    Ok(QueryOutput::Conventions(Conventions {
        language: result.language,
        error_handling: result.error_handling,
        naming: result.naming,
        test_patterns: result.test_patterns,
        common_imports: strings(result.common_imports, budget),
        file_organization: result.file_organization,
        complexity: result.complexity,
    }))
}

/// The MCP `edit_plan` handler body over the captured publication: one
/// capture, so plan structure and co-change data agree.
pub(super) fn edit_plan(
    live: &LiveIndex,
    temporal: &crate::live_index::git_temporal::GitTemporalIndex,
    target: &str,
    budget: &mut Budget,
) -> Result<QueryOutput, QueryRefusalKind> {
    budget.check()?;
    let rendered = super::guidance::edit_plan::plan_edit(live, temporal, target);
    budget.required(target.len() + rendered.len())?;
    Ok(QueryOutput::EditPlan(EditPlanGuidance {
        target: target.to_owned(),
        rendered,
    }))
}

pub(super) fn explore(
    live: &LiveIndex,
    options: &ExploreRequest,
    budget: &mut Budget,
) -> Result<QueryOutput, QueryRefusalKind> {
    use super::guidance::exploration;
    // Same depth-keyed figure the MCP handler returns before touching the index.
    if options.estimate == Some(true) {
        budget.required(0)?;
        return Ok(QueryOutput::SearchEstimate(SearchEstimate {
            approximate_tokens: match options.depth {
                1 => 500,
                2 => 1500,
                _ => 3000,
            },
        }));
    }
    let request = exploration::ExploreRequest {
        query: options.query.clone(),
        limit: options.limit as usize,
        depth: Some(options.depth),
        include_noise: options.include_noise,
        include_vendor: options.include_vendor,
        include_personal_tooling: options.include_personal_tooling,
        language: super::guidance::filters::parse_language_filter(options.language.as_deref())
            .map_err(|_| QueryRefusalKind::UnsupportedOption)?,
        path_prefix: options.path_prefix.clone(),
    };
    let result = exploration::explore(live, &request, &mut || budget.check().is_ok());
    budget.check()?;
    let result = result.map_err(|_| QueryRefusalKind::InvalidRequest)?;
    // A token cap bounds what is returned, never what is retrievable: the
    // full projection is cached first, exactly like the MCP CCR budget.
    if options.max_tokens.is_some() {
        let mut full = budget.projection_budget();
        budget.cache_output = Some(project_exploration(live, &result, &mut full)?);
        budget.cache_truncated = full.truncated;
        budget.check()?;
        budget.cap_tokens(options.max_tokens);
    }
    project_exploration(live, &result, budget)
}

fn project_exploration(
    live: &LiveIndex,
    result: &super::guidance::exploration::ExploreResult,
    budget: &mut Budget,
) -> Result<QueryOutput, QueryRefusalKind> {
    budget.required(result.display_label.len())?;
    budget.truncated |= result.overflow_count > 0;
    let mut symbols = Vec::new();
    for ((name, kind, path), score) in result.symbol_hits.iter().zip(&result.symbol_scores) {
        let enriched = result.enriched_symbols.iter().find(
            |(candidate, candidate_kind, candidate_path, _, _)| {
                candidate == name && candidate_kind == kind && candidate_path == path
            },
        );
        let signature = enriched.and_then(|(_, _, _, signature, _)| signature.clone());
        if !budget
            .row(name.len() + kind.len() + path.len() + signature.as_ref().map_or(0, String::len))
        {
            break;
        }
        let dependent_files = strings(
            enriched
                .into_iter()
                .flat_map(|(_, _, _, _, files)| files.iter().cloned()),
            budget,
        );
        let implementations = strings(
            result
                .symbol_impls
                .iter()
                .filter(|(candidate, _)| candidate == name)
                .flat_map(|(_, values)| values.iter().cloned()),
            budget,
        );
        let type_dependencies = strings(
            result
                .symbol_deps
                .iter()
                .filter(|(candidate, _)| candidate == name)
                .flat_map(|(_, values)| values.iter().cloned()),
            budget,
        );
        symbols.push(ExploreSymbol {
            name: name.clone(),
            kind: kind.clone(),
            path: path.clone(),
            score_millionths: (score * 1_000_000.0).round() as u32,
            signature,
            dependent_files,
            implementations,
            type_dependencies,
        });
    }
    let mut text_matches = Vec::new();
    for (path, preview, line) in &result.text_hits {
        let Some(row) = text_line(live, path, *line as u32, preview.clone(), budget)? else {
            break;
        };
        text_matches.push(row);
    }
    let mut related_files = Vec::new();
    for (path, matches) in &result.related_files {
        if !budget.row(path.len()) {
            break;
        }
        related_files.push(RelatedFile {
            path: path.clone(),
            matches: *matches as u64,
        });
    }
    let (derived_seed_terms, derived_symbols, derived_seed_files) =
        if let Some(cluster) = &result.derived_cluster {
            (
                strings(cluster.seed_terms.iter().cloned(), budget),
                strings(cluster.promoted_symbols.iter().cloned(), budget),
                strings(cluster.seed_files.iter().cloned(), budget),
            )
        } else {
            (Vec::new(), Vec::new(), Vec::new())
        };
    let mut exploration = Exploration {
        label: result.display_label.clone(),
        depth: result.depth,
        symbols,
        text_matches,
        related_files,
        derived_seed_terms,
        derived_symbols,
        derived_seed_files,
        enriched_imports: strings(result.enriched_imports.iter().cloned(), budget),
        hidden_noise_results: result.noise_hidden as u64,
        rendered: String::new(),
    };
    exploration.rendered = super::embed_ask::exploration_text(&exploration);
    Ok(QueryOutput::Exploration(exploration))
}
