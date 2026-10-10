//! Bounded public projections of the same guidance used by MCP.

use super::embed_query::{Budget, text_line};
use crate::embed::parity::guidance::{
    Conventions, Exploration, ExploreRequest, ExploreSymbol, RelatedFile,
};
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

pub(super) fn explore(
    live: &LiveIndex,
    options: &ExploreRequest,
    budget: &mut Budget,
) -> Result<QueryOutput, QueryRefusalKind> {
    use super::guidance::exploration;
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
    budget.required(result.display_label.len())?;
    budget.truncated |= result.overflow_count > 0;
    let mut symbols = Vec::new();
    for ((name, kind, path), score) in result.symbol_hits.into_iter().zip(result.symbol_scores) {
        let enriched = result.enriched_symbols.iter().find(
            |(candidate, candidate_kind, candidate_path, _, _)| {
                candidate == &name && candidate_kind == &kind && candidate_path == &path
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
                .filter(|(candidate, _)| candidate == &name)
                .flat_map(|(_, values)| values.iter().cloned()),
            budget,
        );
        let type_dependencies = strings(
            result
                .symbol_deps
                .iter()
                .filter(|(candidate, _)| candidate == &name)
                .flat_map(|(_, values)| values.iter().cloned()),
            budget,
        );
        symbols.push(ExploreSymbol {
            name,
            kind,
            path,
            score_millionths: (score * 1_000_000.0).round() as u32,
            signature,
            dependent_files,
            implementations,
            type_dependencies,
        });
    }
    let mut text_matches = Vec::new();
    for (path, preview, line) in result.text_hits {
        let Some(row) = text_line(live, &path, line as u32, preview, budget)? else {
            break;
        };
        text_matches.push(row);
    }
    let mut related_files = Vec::new();
    for (path, matches) in result.related_files {
        if !budget.row(path.len()) {
            break;
        }
        related_files.push(RelatedFile {
            path,
            matches: matches as u64,
        });
    }
    let (derived_seed_terms, derived_symbols, derived_seed_files) =
        if let Some(cluster) = result.derived_cluster {
            (
                strings(cluster.seed_terms, budget),
                strings(cluster.promoted_symbols, budget),
                strings(cluster.seed_files, budget),
            )
        } else {
            (Vec::new(), Vec::new(), Vec::new())
        };
    Ok(QueryOutput::Exploration(Exploration {
        label: result.display_label,
        depth: result.depth,
        symbols,
        text_matches,
        related_files,
        derived_seed_terms,
        derived_symbols,
        derived_seed_files,
        enriched_imports: strings(result.enriched_imports, budget),
        hidden_noise_results: result.noise_hidden as u64,
    }))
}
